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
}

#[cfg(any(windows, test))]
fn owned_creation_flags(no_window: bool) -> u32 {
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
        collections::BTreeMap,
        ffi::{OsStr, OsString},
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
        Security::SECURITY_ATTRIBUTES,
        System::{
            JobObjects::AssignProcessToJobObject,
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
        let stdout = if matches!(io, ChildIo::PythonBridge) {
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
        if ResumeThread(info.hThread) == u32::MAX {
            let error = last_error("Unable to resume owned process after Job assignment");
            TerminateProcess(info.hProcess, 1);
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hThread);
            CloseHandle(info.hProcess);
            close_parents(&stdin, &stdout, &stderr);
            return Err(error);
        }
        CloseHandle(info.hThread);
        Ok(OwnedChild {
            process: info.hProcess,
            stdin: stdin.map(|p| File::from_raw_handle(p.parent as _)),
            stdout: stdout.map(|p| File::from_raw_handle(p.parent as _)),
            stderr: stderr.map(|p| File::from_raw_handle(p.parent as _)),
        })
    }

    impl OwnedChild {
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
    }
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.process);
            }
        }
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
        let mut vars: BTreeMap<String, OsString> = std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_uppercase(),
                    OsString::from(format!("{}={}", k.to_string_lossy(), v.to_string_lossy())),
                )
            })
            .collect();
        for (k, v) in command.get_envs() {
            let key = k.to_string_lossy().to_uppercase();
            if let Some(v) = v {
                vars.insert(
                    key,
                    OsString::from(format!("{}={}", k.to_string_lossy(), v.to_string_lossy())),
                );
            } else {
                vars.remove(&key);
            }
        }
        let mut block = Vec::new();
        for item in vars.values() {
            block.extend(item.encode_wide());
            block.push(0);
        }
        block.push(0);
        block
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
