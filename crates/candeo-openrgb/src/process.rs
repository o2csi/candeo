//! The OpenRGB Candeo starts, tied to Candeo's lifetime
//! (`openrgb-output.md` §6.5).
//!
//! - **Windows**: a job object that kills its processes when its last handle
//!   closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), the child assigned to it
//!   as soon as it exists. Candeo holds the only handle, so however Candeo
//!   ends, Windows closes it and the child goes. Inside the Store package
//!   Candeo is itself in a job; jobs nest since Windows 8. A child that cannot
//!   be assigned is killed: none runs untied.
//! - **Linux**: `PR_SET_PDEATHSIG` with `SIGTERM`, set in the child before
//!   `exec`, and a process group of its own. The signal follows the *thread*
//!   that started the child: start it from a thread that lives as long as
//!   the output.
//!
//! Its standard streams go nowhere: what OpenRGB prints may carry device
//! paths, which Candeo's log must not.

use std::io;
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::launch::LaunchCommand;

/// On Windows, starts `command` without a console window.
pub(crate) fn no_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub struct ManagedChild {
    child: Child,
    #[cfg(windows)]
    job: windows::Job,
    stopped: bool,
}

impl ManagedChild {
    pub fn spawn(launch: &LaunchCommand) -> io::Result<Self> {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        no_window(&mut command);
        Self::start(command)
    }

    #[cfg(windows)]
    fn start(mut command: Command) -> io::Result<Self> {
        let job = windows::Job::new()?;
        let mut child = command.spawn()?;
        if let Err(e) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        Ok(Self {
            child,
            job,
            stopped: false,
        })
    }

    #[cfg(target_os = "linux")]
    fn start(mut command: Command) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;
        let parent = libc::pid_t::try_from(std::process::id()).unwrap_or(0);
        command.process_group(0);
        // SAFETY: between `fork` and `exec` the closure calls only `prctl`
        // and `getppid`, both async-signal-safe, and allocates nothing.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                // Candeo ended before the signal was armed.
                if libc::getppid() != parent {
                    return Err(io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        Ok(Self {
            child: command.spawn()?,
            stopped: false,
        })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn start(_: Command) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows and Linux only",
        ))
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Its exit status, once it has exited on its own.
    pub fn exited(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Stops it: the job terminated on Windows; `SIGTERM` to its group on
    /// Linux, then `SIGKILL` after three seconds.
    pub fn stop(mut self) -> io::Result<ExitStatus> {
        self.halt()
    }

    fn halt(&mut self) -> io::Result<ExitStatus> {
        self.stopped = true;
        if let Some(status) = self.child.try_wait()? {
            return Ok(status);
        }
        #[cfg(windows)]
        self.job.terminate()?;
        #[cfg(target_os = "linux")]
        {
            let group = libc::pid_t::try_from(self.child.id()).unwrap_or(0);
            if group > 0 {
                // SAFETY: a signal to the child's own process group.
                unsafe { libc::kill(-group, libc::SIGTERM) };
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if let Some(status) = self.child.try_wait()? {
                    return Ok(status);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            if group > 0 {
                // SAFETY: as above.
                unsafe { libc::kill(-group, libc::SIGKILL) };
            }
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        self.child.kill()?;
        self.child.wait()
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.halt();
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub(super) struct Job(HANDLE);

    // SAFETY: a job handle may be used from any thread.
    unsafe impl Send for Job {}

    impl Job {
        pub(super) fn new() -> io::Result<Self> {
            // SAFETY: no name and no security attributes; the handle is
            // checked, then owned by `Job`.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle);
            // SAFETY: a zeroed structure is a valid "no limits" value.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: the structure and its size match the information class.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&limits).cast(),
                    u32::try_from(std::mem::size_of_val(&limits)).unwrap_or(0),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub(super) fn assign(&self, child: &Child) -> io::Result<()> {
            // SAFETY: both handles are valid for the call.
            if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) fn terminate(&self) -> io::Result<()> {
            // SAFETY: the handle is valid until `drop`.
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        #[cfg(test)]
        pub(super) fn holds(&self, child: &Child) -> bool {
            use windows_sys::Win32::System::JobObjects::IsProcessInJob;
            let mut inside = 0;
            // SAFETY: both handles are valid for the call.
            let ok = unsafe { IsProcessInJob(child.as_raw_handle(), self.0, &mut inside) };
            ok != 0 && inside != 0
        }
    }

    impl Drop for Job {
        /// The last handle: the kill-on-close limit ends whatever is left.
        fn drop(&mut self) {
            // SAFETY: owned, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// Something harmless that runs for half a minute, in place of OpenRGB.
    fn long_runner() -> LaunchCommand {
        if cfg!(windows) {
            LaunchCommand {
                program: PathBuf::from("ping"),
                args: ["-n", "30", "127.0.0.1"]
                    .iter()
                    .map(OsString::from)
                    .collect(),
            }
        } else {
            LaunchCommand {
                program: PathBuf::from("sleep"),
                args: vec![OsString::from("30")],
            }
        }
    }

    #[test]
    fn stopping_ends_the_child_at_once() {
        let mut child = ManagedChild::spawn(&long_runner()).unwrap();
        assert!(child.exited().unwrap().is_none(), "still running");
        let started = Instant::now();
        let status = child.stop().unwrap();
        assert!(!status.success());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn dropping_the_handle_ends_the_child() {
        let child = ManagedChild::spawn(&long_runner()).unwrap();
        let id = child.id();
        drop(child);
        assert!(!running(id), "process {id} outlived its handle");
    }

    #[cfg(windows)]
    #[test]
    fn the_child_is_in_the_job_from_the_start() {
        let child = ManagedChild::spawn(&long_runner()).unwrap();
        assert!(child.job.holds(&child.child));
    }

    #[cfg(windows)]
    fn running(id: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: the handle is checked and closed.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE as u32
        }
    }

    #[cfg(not(windows))]
    fn running(id: u32) -> bool {
        // Reaped by `wait` in `drop`: no zombie, no entry.
        std::path::Path::new(&format!("/proc/{id}")).exists()
    }
}
