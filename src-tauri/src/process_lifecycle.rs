use std::{
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

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

    pub(crate) fn spawn(&self, command: &mut Command) -> Result<Child, String> {
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
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        #[cfg(windows)]
        unsafe {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
            if AssignProcessToJobObject(self.job, child.as_raw_handle() as _) == 0 {
                let error = std::io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Unable to assign an owned process to the AutoCoder job: {error}"
                ));
            }
        }
        Ok(child)
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
