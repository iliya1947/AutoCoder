use std::{
    process::{Command, ExitStatus, Output},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[cfg(not(windows))]
pub(crate) type OwnedChild = std::process::Child;

#[derive(Clone, Copy)]
pub(crate) enum ChildIo {
    Ollama,
    PythonBridge,
    Terminal,
}

#[cfg(any(windows, test))]
pub(crate) fn owned_creation_flags(no_window: bool) -> u32 {
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | if no_window { CREATE_NO_WINDOW } else { 0 }
}

/// The single launch/ownership boundary for processes whose lifetime is bounded by AutoCoder.
pub(crate) struct ProcessLifecycle {
    accepting: AtomicBool,
    launch_gate: Mutex<()>,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

impl ProcessLifecycle {
    pub(crate) fn new() -> Result<Self, String> {
        #[cfg(windows)]
        unsafe {
            use std::{
                mem::{size_of, zeroed},
                ptr::null,
            };
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::JobObjects::{
                    CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
            };
            let job = CreateJobObjectW(null(), null());
            if job.is_null() {
                return Err(format!(
                    "Unable to create the AutoCoder process job: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(format!(
                    "Unable to configure the AutoCoder process job: {error}"
                ));
            }
            return Ok(Self {
                accepting: AtomicBool::new(true),
                launch_gate: Mutex::new(()),
                job,
            });
        }
        #[cfg(not(windows))]
        Ok(Self {
            accepting: AtomicBool::new(true),
            launch_gate: Mutex::new(()),
        })
    }

    pub(crate) fn spawn(&self, command: &mut Command, io: ChildIo) -> Result<OwnedChild, String> {
        let _launch = self
            .launch_gate
            .lock()
            .map_err(|_| "Unable to access the owned process launcher.".to_string())?;
        if !self.accepting.load(Ordering::Acquire) {
            return Err(
                "AutoCoder is shutting down and cannot start another service process.".into(),
            );
        }
        super::configure_child_process(command);
        #[cfg(not(windows))]
        return command.spawn().map_err(|error| error.to_string());
        #[cfg(windows)]
        return unsafe { windows::spawn_suspended_in_job(command, io, self.job) };
    }

    pub(crate) fn shutdown(&self) {
        let Ok(_launch) = self.launch_gate.lock() else {
            return;
        };
        if self.accepting.swap(false, Ordering::AcqRel) {
            #[cfg(windows)]
            unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_accepting(&self) -> bool {
        self.accepting.load(Ordering::Acquire)
    }
}

impl Drop for ProcessLifecycle {
    fn drop(&mut self) {
        self.shutdown();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}
unsafe impl Send for ProcessLifecycle {}
unsafe impl Sync for ProcessLifecycle {}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        cmp::Ordering as CmpOrdering,
        ffi::OsStr,
        fs::File,
        io::Read,
        mem::{size_of, zeroed},
        os::windows::{ffi::OsStrExt, io::FromRawHandle, process::ExitStatusExt},
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
            STILL_ACTIVE, WAIT_OBJECT_0,
        },
        Globalization::{CompareStringOrdinal, CSTR_EQUAL, CSTR_LESS_THAN},
        Security::SECURITY_ATTRIBUTES,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Pipes::CreatePipe,
            Threading::{
                CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess,
                WaitForSingleObject, CREATE_NO_WINDOW, INFINITE, PROCESS_INFORMATION,
                STARTF_USESTDHANDLES, STARTUPINFOW,
            },
        },
    };

    pub(crate) struct OwnedChild {
        process: HANDLE,
        terminal_job: Option<HANDLE>,
        pub(crate) stdin: Option<File>,
        pub(crate) stdout: Option<File>,
        pub(crate) stderr: Option<File>,
    }
    unsafe impl Send for OwnedChild {}

    struct RawPipe {
        parent: HANDLE,
        child: HANDLE,
    }
    impl RawPipe {
        unsafe fn new(parent_reads: bool) -> Result<Self, String> {
            let mut read = null_mut();
            let mut write = null_mut();
            let mut security = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: null_mut(),
                bInheritHandle: 1,
            };
            if CreatePipe(&mut read, &mut write, &mut security, 0) == 0 {
                return Err(last_error("Unable to create child pipe"));
            }
            let (parent, child) = if parent_reads {
                (read, write)
            } else {
                (write, read)
            };
            if SetHandleInformation(parent, HANDLE_FLAG_INHERIT, 0) == 0 {
                CloseHandle(read);
                CloseHandle(write);
                return Err(last_error("Unable to protect parent pipe handle"));
            }
            Ok(Self { parent, child })
        }
    }

    pub(super) unsafe fn spawn_suspended_in_job(
        command: &Command,
        io: ChildIo,
        job: HANDLE,
    ) -> Result<OwnedChild, String> {
        let stdin = if matches!(io, ChildIo::PythonBridge) {
            Some(RawPipe::new(false)?)
        } else {
            None
        };
        let stdout = if matches!(io, ChildIo::PythonBridge | ChildIo::Terminal) {
            Some(RawPipe::new(true)?)
        } else {
            None
        };
        let stderr = Some(RawPipe::new(true)?);
        let mut startup: STARTUPINFOW = zeroed();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = stdin.as_ref().map_or(null_mut(), |p| p.child);
        startup.hStdOutput = stdout.as_ref().map_or(null_mut(), |p| p.child);
        startup.hStdError = stderr.as_ref().unwrap().child;
        let mut info: PROCESS_INFORMATION = zeroed();
        let mut cmdline = command_line(command);
        let mut environment = environment_block(command);
        let cwd = command
            .get_current_dir()
            .map(|path| wide_nul(path.as_os_str()));
        let flags = owned_creation_flags(creation_flags() != 0);
        let created = CreateProcessW(
            null(),
            cmdline.as_mut_ptr(),
            null(),
            null(),
            1,
            flags,
            environment.as_mut_ptr() as _,
            cwd.as_ref().map_or(null(), |v| v.as_ptr()),
            &startup,
            &mut info,
        );
        for pipe in [&stdin, &stdout, &stderr].into_iter().flatten() {
            CloseHandle(pipe.child);
        }
        if created == 0 {
            close_parents(&stdin, &stdout, &stderr);
            return Err(last_error("Unable to create owned process suspended"));
        }
        if AssignProcessToJobObject(job, info.hProcess) == 0 {
            let error = last_error("Unable to assign suspended process to the AutoCoder job");
            TerminateProcess(info.hProcess, 1);
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hThread);
            CloseHandle(info.hProcess);
            close_parents(&stdin, &stdout, &stderr);
            return Err(error);
        }
        let terminal_job = if matches!(io, ChildIo::Terminal) {
            let terminal_job = CreateJobObjectW(null(), null());
            if terminal_job.is_null() {
                let error = last_error("Unable to create the terminal command job");
                terminate_failed_launch(info, &stdin, &stdout, &stderr);
                return Err(error);
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                terminal_job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
                || AssignProcessToJobObject(terminal_job, info.hProcess) == 0
            {
                let error = last_error("Unable to isolate the terminal command in its own job");
                CloseHandle(terminal_job);
                terminate_failed_launch(info, &stdin, &stdout, &stderr);
                return Err(error);
            }
            Some(terminal_job)
        } else {
            None
        };
        if ResumeThread(info.hThread) == u32::MAX {
            let error = last_error("Unable to resume owned process after Job assignment");
            TerminateProcess(info.hProcess, 1);
            WaitForSingleObject(info.hProcess, INFINITE);
            if let Some(job) = terminal_job {
                CloseHandle(job);
            }
            CloseHandle(info.hThread);
            CloseHandle(info.hProcess);
            close_parents(&stdin, &stdout, &stderr);
            return Err(error);
        }
        CloseHandle(info.hThread);
        Ok(OwnedChild {
            process: info.hProcess,
            terminal_job,
            stdin: stdin.map(|p| File::from_raw_handle(p.parent as _)),
            stdout: stdout.map(|p| File::from_raw_handle(p.parent as _)),
            stderr: stderr.map(|p| File::from_raw_handle(p.parent as _)),
        })
    }

    impl OwnedChild {
        pub(crate) fn cancel_tree(&mut self) -> std::io::Result<()> {
            unsafe {
                if let Some(job) = self.terminal_job {
                    if TerminateJobObject(job, 1) == 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                } else {
                    self.kill()
                }
            }
        }
        pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
            unsafe {
                let mut code = 0;
                if GetExitCodeProcess(self.process, &mut code) == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok((code != STILL_ACTIVE as u32).then(|| ExitStatus::from_raw(code)))
            }
        }
        pub(crate) fn kill(&mut self) -> std::io::Result<()> {
            unsafe {
                if TerminateProcess(self.process, 1) == 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }
        pub(crate) fn wait(&mut self) -> std::io::Result<ExitStatus> {
            unsafe {
                if WaitForSingleObject(self.process, INFINITE) != WAIT_OBJECT_0 {
                    return Err(std::io::Error::last_os_error());
                }
                self.try_wait()?.ok_or_else(std::io::Error::last_os_error)
            }
        }
        pub(crate) fn wait_with_output(mut self) -> std::io::Result<Output> {
            drop(self.stdin.take());
            let out = self.stdout.take().map(|mut f| {
                std::thread::spawn(move || {
                    let mut v = Vec::new();
                    f.read_to_end(&mut v).map(|_| v)
                })
            });
            let err = self.stderr.take().map(|mut f| {
                std::thread::spawn(move || {
                    let mut v = Vec::new();
                    f.read_to_end(&mut v).map(|_| v)
                })
            });
            let status = self.wait()?;
            let stdout = out.map_or(Ok(Vec::new()), |h| {
                h.join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("stdout reader panicked")))
            })?;
            let stderr = err.map_or(Ok(Vec::new()), |h| {
                h.join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("stderr reader panicked")))
            })?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }

        pub(crate) fn wait_with_output_cancelled(
            mut self,
            cancel: &AtomicBool,
        ) -> std::io::Result<(Output, bool)> {
            drop(self.stdin.take());
            let out = self.stdout.take().map(|mut file| {
                std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes).map(|_| bytes)
                })
            });
            let err = self.stderr.take().map(|mut file| {
                std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes).map(|_| bytes)
                })
            });
            let mut cancelled = false;
            let status = loop {
                if cancel.load(Ordering::Acquire) && !cancelled {
                    self.cancel_tree()?;
                    cancelled = true;
                }
                if let Some(status) = self.try_wait()? {
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            };
            let stdout = out.map_or(Ok(Vec::new()), |handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("stdout reader panicked")))
            })?;
            let stderr = err.map_or(Ok(Vec::new()), |handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(std::io::Error::other("stderr reader panicked")))
            })?;
            Ok((
                Output {
                    status,
                    stdout,
                    stderr,
                },
                cancelled,
            ))
        }
    }
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            unsafe {
                if let Some(job) = self.terminal_job {
                    CloseHandle(job);
                }
                CloseHandle(self.process);
            }
        }
    }

    unsafe fn terminate_failed_launch(
        info: PROCESS_INFORMATION,
        stdin: &Option<RawPipe>,
        stdout: &Option<RawPipe>,
        stderr: &Option<RawPipe>,
    ) {
        TerminateProcess(info.hProcess, 1);
        WaitForSingleObject(info.hProcess, INFINITE);
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
        close_parents(stdin, stdout, stderr);
    }

    unsafe fn close_parents(a: &Option<RawPipe>, b: &Option<RawPipe>, c: &Option<RawPipe>) {
        for p in [a, b, c].into_iter().flatten() {
            CloseHandle(p.parent);
        }
    }
    fn last_error(context: &str) -> String {
        format!(
            "{context}: {} (Win32 {})",
            std::io::Error::last_os_error(),
            unsafe { GetLastError() }
        )
    }
    fn wide_nul(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    fn quote(value: &OsStr) -> String {
        let s = value.to_string_lossy();
        if !s.contains([' ', '\t', '"']) {
            return s.into_owned();
        }
        let mut out = String::from("\"");
        let mut slashes = 0;
        for c in s.chars() {
            if c == '\\' {
                slashes += 1
            } else {
                if c == '"' {
                    out.push_str(&"\\".repeat(slashes * 2 + 1));
                } else {
                    out.push_str(&"\\".repeat(slashes));
                }
                slashes = 0;
                out.push(c)
            }
        }
        out.push_str(&"\\".repeat(slashes * 2));
        out.push('"');
        out
    }
    fn command_line(command: &Command) -> Vec<u16> {
        let mut text = quote(command.get_program());
        for arg in command.get_args() {
            text.push(' ');
            text.push_str(&quote(arg));
        }
        OsStr::new(&text).encode_wide().chain(Some(0)).collect()
    }
    fn environment_block(command: &Command) -> Vec<u16> {
        let inherited = std::env::vars_os()
            .map(|(key, value)| (key.encode_wide().collect(), value.encode_wide().collect()));
        let overrides = command.get_envs().map(|(key, value)| {
            (
                key.encode_wide().collect(),
                value.map(|value| value.encode_wide().collect()),
            )
        });
        environment_block_from(inherited, overrides)
    }

    fn environment_block_from(
        inherited: impl IntoIterator<Item = (Vec<u16>, Vec<u16>)>,
        overrides: impl IntoIterator<Item = (Vec<u16>, Option<Vec<u16>>)>,
    ) -> Vec<u16> {
        let mut vars: Vec<(Vec<u16>, Vec<u16>)> = Vec::new();
        for (key, value) in inherited {
            set_environment_entry(&mut vars, key, Some(value));
        }
        for (key, value) in overrides {
            set_environment_entry(&mut vars, key, value);
        }
        vars.sort_by(|left, right| compare_environment_keys(&left.0, &right.0));
        let mut block = Vec::new();
        for (key, value) in vars {
            block.extend(key);
            block.push(b'=' as u16);
            block.extend(value);
            block.push(0);
        }
        block.push(0);
        block
    }

    fn set_environment_entry(
        vars: &mut Vec<(Vec<u16>, Vec<u16>)>,
        key: Vec<u16>,
        value: Option<Vec<u16>>,
    ) {
        let existing = vars
            .iter()
            .position(|entry| compare_environment_keys(&entry.0, &key) == CmpOrdering::Equal);
        match (existing, value) {
            (Some(index), Some(value)) => vars[index] = (key, value),
            (None, Some(value)) => vars.push((key, value)),
            (Some(index), None) => {
                vars.remove(index);
            }
            (None, None) => {}
        }
    }

    fn compare_environment_keys(left: &[u16], right: &[u16]) -> CmpOrdering {
        let result = unsafe {
            CompareStringOrdinal(
                left.as_ptr(),
                left.len() as i32,
                right.as_ptr(),
                right.len() as i32,
                1,
            )
        };
        match result {
            CSTR_LESS_THAN => CmpOrdering::Less,
            CSTR_EQUAL => CmpOrdering::Equal,
            3 => CmpOrdering::Greater,
            _ => left.cmp(right),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn wide(value: &str) -> Vec<u16> {
            OsStr::new(value).encode_wide().collect()
        }

        #[test]
        fn environment_block_preserves_unicode_and_applies_case_insensitive_overrides() {
            let block = environment_block_from(
                [
                    (wide("ПЕРЕМЕННАЯ"), wide("исходное")),
                    (wide("REMOVE_ME"), wide("x")),
                ],
                [
                    (wide("переменная"), Some(wide("значение-שלום"))),
                    (wide("REMOVE_me"), None),
                    (wide("PYTHONUTF8"), Some(wide("1"))),
                ],
            );
            let expected: Vec<u16> = "PYTHONUTF8=1\0переменная=значение-שלום\0\0"
                .encode_utf16()
                .collect();
            assert_eq!(block, expected);
        }
    }
    fn creation_flags() -> u32 {
        let show = std::env::var_os("AUTOCODER_SHOW_CHILD_CONSOLES").is_some_and(|v| v != "0");
        if !cfg!(debug_assertions) && !show {
            CREATE_NO_WINDOW
        } else {
            0
        }
    }
}

#[cfg(windows)]
pub(crate) use windows::OwnedChild;
