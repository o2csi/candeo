//! The local pipe between `candeo --mcp` and the server inside Candeo
//! (`docs/design/mcp-server.md` §2.4), which only the signed-in user can open.
//!
//! - **Windows**: a named pipe. Named pipes are global to the machine and two
//!   people may be signed in, so its name carries the user's SID. It is created
//!   as the first instance of that name, refuses remote clients, and its DACL
//!   grants that user alone: the default one also lets Everyone read.
//! - **Linux and macOS**: a Unix socket of mode 0600 in a directory of mode 0700,
//!   `$XDG_RUNTIME_DIR/candeo/` or the application's data folder. Each side
//!   checks the other's user id at each connection.
//!
//! The bridge checks who serves the pipe too. Anyone may create a named pipe
//! of any free name, so another account signed in on the same computer could
//! take the name while Candeo is not running, and read what the assistant sends
//! or answer in Candeo's place. A pipe owned by another user is refused.
//!
//! The name and the path are never logged: one holds the SID, the other the
//! user's name.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

/// Connections served at once (§2.5). A fifth connects and waits for one of them
/// to close.
pub(crate) const MAX_CONNECTIONS: usize = 4;

/// A connection, either side of it.
pub(crate) trait Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> Stream for T {}

/// Why listening did not start.
#[derive(Debug)]
pub(crate) enum BindError {
    /// Something else holds the name: nothing listens, and Settings says so.
    Taken,
    Failed(io::Error),
}

/// Why the bridge could not connect.
#[derive(Debug)]
pub(crate) enum Unreachable {
    /// No pipe: Candeo is not running, still starting, or MCP is off.
    Absent,
    /// Every instance is in use: the bridge waits for one. A Unix socket keeps
    /// accepting, and never says so.
    #[cfg_attr(not(windows), allow(dead_code))]
    Busy,
    /// Served by a process of another user.
    NotOurs,
    Other(io::Error),
}

#[cfg(windows)]
pub(crate) use self::windows::*;

#[cfg(unix)]
pub(crate) use self::unix::*;

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;

    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use super::{BindError, Unreachable, MAX_CONNECTIONS};

    /// The signed-in user's token user, as Windows returns it: the SID lives
    /// inside the buffer.
    struct UserSid(Vec<u8>);

    impl UserSid {
        fn current() -> io::Result<Self> {
            let mut token: HANDLE = ptr::null_mut();
            // SAFETY: the pseudo-handle of the current process needs no closing;
            // `token` receives a handle closed below.
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut length = 0u32;
            // SAFETY: a null buffer asks for the length only.
            unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut length) };
            let mut buffer = vec![0u8; length as usize];
            // SAFETY: the buffer holds `length` bytes, as just asked for.
            let read = unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    buffer.as_mut_ptr().cast(),
                    length,
                    &mut length,
                )
            };
            let error = io::Error::last_os_error();
            // SAFETY: `token` is the handle opened above.
            unsafe { CloseHandle(token) };
            if read == 0 {
                return Err(error);
            }
            Ok(Self(buffer))
        }

        fn sid(&self) -> PSID {
            // SAFETY: the buffer was filled with a TOKEN_USER by Windows, and
            // stays alive as long as `self`.
            unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
        }

        fn text(&self) -> io::Result<String> {
            sid_text(self.sid())
        }
    }

    /// A SID as text: `S-1-5-21-…`.
    fn sid_text(sid: PSID) -> io::Result<String> {
        let mut wide: *mut u16 = ptr::null_mut();
        // SAFETY: `sid` is a valid SID; Windows allocates the string, freed below.
        if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a NUL-terminated string Windows just wrote.
        let text = unsafe {
            let length = (0..).take_while(|&i| *wide.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(wide, length))
        };
        // SAFETY: allocated by `ConvertSidToStringSidW` with `LocalAlloc`.
        unsafe { LocalFree(wide.cast()) };
        Ok(text)
    }

    /// The pipe's name for the signed-in user.
    pub(crate) fn name() -> io::Result<String> {
        Ok(name_for(&UserSid::current()?.text()?))
    }

    pub(crate) fn name_for(sid: &str) -> String {
        format!(r"\\.\pipe\candeo-mcp-{sid}")
    }

    /// The pipe's security descriptor: a protected DACL, so nothing is inherited,
    /// holding one entry that grants the user everything. No other account, no
    /// AppContainer: a browser's sandboxed process reaches nothing.
    pub(crate) fn sddl(sid: &str) -> String {
        format!("D:P(A;;GA;;;{sid})")
    }

    /// A security descriptor Windows allocated, freed with it.
    struct Descriptor(PSECURITY_DESCRIPTOR);

    // SAFETY: the descriptor is only read once made, by `CreateNamedPipeW`.
    unsafe impl Send for Descriptor {}
    unsafe impl Sync for Descriptor {}

    impl Descriptor {
        fn from_sddl(sddl: &str) -> io::Result<Self> {
            let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
            // SAFETY: `wide` is NUL-terminated; Windows allocates the descriptor.
            let made = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            };
            if made == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    impl Drop for Descriptor {
        fn drop(&mut self) {
            // SAFETY: allocated with `LocalAlloc` by the conversion above.
            unsafe { LocalFree(self.0) };
        }
    }

    /// Instances of the pipe that may exist at once: the connections served,
    /// and one more, waiting for a client or connected and waiting for a slot.
    const INSTANCES: usize = MAX_CONNECTIONS + 1;

    /// The server's side.
    ///
    /// One instance always exists while it listens, the one waiting for the next
    /// client: the next one is created before that client is handed over. Were
    /// there a moment with none, another process could create the name, and the
    /// next instance created here would join its pipe.
    pub(crate) struct Listener {
        name: String,
        descriptor: Descriptor,
        waiting: Option<NamedPipeServer>,
    }

    impl Listener {
        pub(crate) fn bind() -> Result<Self, BindError> {
            Self::bind_at(&name().map_err(BindError::Failed)?)
        }

        /// Listens on `name`, which must not exist yet.
        pub(crate) fn bind_at(name: &str) -> Result<Self, BindError> {
            let sid = UserSid::current()
                .and_then(|user| user.text())
                .map_err(BindError::Failed)?;
            let descriptor = Descriptor::from_sddl(&sddl(&sid)).map_err(BindError::Failed)?;
            let mut listener = Self {
                name: name.to_owned(),
                descriptor,
                waiting: None,
            };
            listener.waiting = Some(listener.instance(true).map_err(|e| {
                // The first instance of a name that exists: someone holds it.
                if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                    BindError::Taken
                } else {
                    BindError::Failed(e)
                }
            })?);
            Ok(listener)
        }

        fn instance(&self, first: bool) -> io::Result<NamedPipeServer> {
            let mut attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.descriptor.0,
                bInheritHandle: 0,
            };
            // SAFETY: `attributes` is a valid SECURITY_ATTRIBUTES whose
            // descriptor outlives the call.
            unsafe {
                ServerOptions::new()
                    .first_pipe_instance(first)
                    .reject_remote_clients(true)
                    .max_instances(INSTANCES)
                    .create_with_security_attributes_raw(
                        &self.name,
                        (&mut attributes as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
                    )
            }
        }

        /// Waits for a client, and hands it over connected. The next client waits
        /// for [`Self::rearm`].
        pub(crate) async fn accept(&mut self) -> io::Result<NamedPipeServer> {
            let server = match self.waiting.take() {
                Some(server) => server,
                None => self.instance(false)?,
            };
            server.connect().await?;
            Ok(server)
        }

        /// Creates the instance the next client connects to. Called while the
        /// client [`Self::accept`] handed over still holds the name.
        pub(crate) fn rearm(&mut self) -> io::Result<()> {
            if self.waiting.is_none() {
                self.waiting = Some(self.instance(false)?);
            }
            Ok(())
        }
    }

    /// The bridge's side: the pipe, opened for overlapped I/O (§2.3), once it
    /// is known to be served by this user.
    pub(crate) async fn connect() -> Result<NamedPipeClient, Unreachable> {
        let name = name().map_err(Unreachable::Other)?;
        connect_to(&name)
    }

    pub(crate) fn connect_to(name: &str) -> Result<NamedPipeClient, Unreachable> {
        let client = ClientOptions::new().open(name).map_err(|e| {
            match e.raw_os_error().map(|code| code as u32) {
                Some(ERROR_FILE_NOT_FOUND) => Unreachable::Absent,
                Some(ERROR_PIPE_BUSY) => Unreachable::Busy,
                _ => Unreachable::Other(e),
            }
        })?;
        match owned_by_user(client.as_raw_handle()) {
            Ok(true) => Ok(client),
            Ok(false) => Err(Unreachable::NotOurs),
            Err(e) => Err(Unreachable::Other(e)),
        }
    }

    /// Whether the pipe behind `handle` belongs to the signed-in user: its
    /// owner is the account of the process that created it.
    pub(crate) fn owned_by_user(handle: std::os::windows::io::RawHandle) -> io::Result<bool> {
        let user = UserSid::current()?;
        let mut owner: PSID = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `handle` is an open pipe; Windows allocates the descriptor
        // `owner` points into, freed below.
        let status = unsafe {
            GetSecurityInfo(
                handle as HANDLE,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        // SAFETY: both SIDs are valid while their buffers live.
        let same = unsafe { EqualSid(owner, user.sid()) } != 0;
        // SAFETY: allocated by `GetSecurityInfo` with `LocalAlloc`.
        unsafe { LocalFree(descriptor) };
        Ok(same)
    }

    #[cfg(test)]
    pub(crate) mod testing {
        use super::*;
        use windows_sys::Win32::Security::{
            AclSizeInformation, GetAce, GetAclInformation, ACCESS_ALLOWED_ACE, ACL,
            ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION,
        };

        pub(crate) fn user_sid() -> String {
            UserSid::current().unwrap().text().unwrap()
        }

        /// The entries of the DACL on the object behind `handle`: whether each
        /// allows, and the SID it names.
        pub(crate) fn dacl(handle: std::os::windows::io::RawHandle) -> Vec<(bool, String)> {
            let mut acl: *mut ACL = ptr::null_mut();
            let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
            // SAFETY: as in `owned_by_user`, for the DACL.
            let status = unsafe {
                GetSecurityInfo(
                    handle as HANDLE,
                    SE_KERNEL_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut acl,
                    ptr::null_mut(),
                    &mut descriptor,
                )
            };
            assert_eq!(status, 0, "GetSecurityInfo failed");
            assert!(!acl.is_null(), "no DACL: everyone would be granted access");
            let mut size = ACL_SIZE_INFORMATION::default();
            // SAFETY: `acl` points into the descriptor, alive until freed below.
            let read = unsafe {
                GetAclInformation(
                    acl,
                    (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            };
            assert_ne!(read, 0, "GetAclInformation failed");
            let mut entries = Vec::new();
            for i in 0..size.AceCount {
                let mut ace: *mut c_void = ptr::null_mut();
                // SAFETY: `i` is below the count Windows gave.
                assert_ne!(unsafe { GetAce(acl, i, &mut ace) }, 0, "GetAce failed");
                // SAFETY: every ACE starts with its header; an allowed or denied
                // ACE then has its mask and its SID, at the same places.
                let (kind, sid) = unsafe {
                    let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
                    (
                        ace.Header.AceType,
                        sid_text((&ace.SidStart as *const u32).cast_mut().cast()).unwrap(),
                    )
                };
                // `ACCESS_ALLOWED_ACE_TYPE` is 0.
                entries.push((kind == 0, sid));
            }
            // SAFETY: allocated by `GetSecurityInfo`.
            unsafe { LocalFree(descriptor) };
            entries
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    use tokio::net::{UnixListener, UnixStream};

    use super::{BindError, Unreachable};

    /// What `tauri.conf.json` names the application: its data folder's name.
    pub(crate) const IDENTIFIER: &str = "com.o2csi.candeo";

    const SOCKET: &str = "mcp.sock";

    /// Where the socket lives, from the environment alone: the bridge computes it
    /// without Tauri, and must find what the server bound.
    ///
    /// `$XDG_RUNTIME_DIR/candeo/` where the session has one, a folder only the
    /// user can open and which the system empties at logout; otherwise `mcp/`
    /// in the application's data folder, as Tauri resolves it.
    pub(crate) fn socket_path(
        runtime_dir: Option<OsString>,
        data_home: Option<OsString>,
        home: Option<OsString>,
    ) -> Option<PathBuf> {
        let absolute =
            |value: Option<OsString>| value.map(PathBuf::from).filter(|path| path.is_absolute());
        if let Some(runtime) = absolute(runtime_dir) {
            return Some(runtime.join("candeo").join(SOCKET));
        }
        let data = if cfg!(target_os = "macos") {
            absolute(home)?.join("Library").join("Application Support")
        } else {
            match absolute(data_home) {
                Some(data) => data,
                None => absolute(home)?.join(".local").join("share"),
            }
        };
        Some(data.join(IDENTIFIER).join("mcp").join(SOCKET))
    }

    fn path() -> io::Result<PathBuf> {
        socket_path(
            std::env::var_os("XDG_RUNTIME_DIR"),
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("HOME"),
        )
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no folder for the MCP socket"))
    }

    fn user() -> u32 {
        // SAFETY: `geteuid` cannot fail and touches nothing.
        unsafe { libc::geteuid() }
    }

    /// The folder holding the socket: made 0700, or made so when it is ours.
    /// One owned by someone else is refused.
    fn private_folder(folder: &Path) -> io::Result<()> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)?;
        let metadata = std::fs::metadata(folder)?;
        if metadata.uid() != user() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the MCP socket's folder belongs to another user",
            ));
        }
        if metadata.mode() & 0o077 != 0 {
            std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    pub(crate) struct Listener {
        listener: UnixListener,
        path: PathBuf,
    }

    impl Listener {
        pub(crate) fn bind() -> Result<Self, BindError> {
            Self::bind_at(&path().map_err(BindError::Failed)?)
        }

        pub(crate) fn bind_at(path: &Path) -> Result<Self, BindError> {
            if let Some(folder) = path.parent() {
                private_folder(folder).map_err(BindError::Failed)?;
            }
            if path.exists() {
                // A socket something answers on is held; one left by a Candeo
                // that did not quit cleanly is removed.
                if std::os::unix::net::UnixStream::connect(path).is_ok() {
                    return Err(BindError::Taken);
                }
                std::fs::remove_file(path).map_err(BindError::Failed)?;
            }
            let listener = UnixListener::bind(path).map_err(BindError::Failed)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(BindError::Failed)?;
            Ok(Self {
                listener,
                path: path.to_owned(),
            })
        }

        /// The next client of this user. One of another user is dropped: the
        /// folder's mode should keep them out, and this holds even if it did not.
        pub(crate) async fn accept(&mut self) -> io::Result<UnixStream> {
            loop {
                let (stream, _) = self.listener.accept().await?;
                match stream.peer_cred() {
                    Ok(peer) if peer.uid() == user() => return Ok(stream),
                    _ => tracing::warn!("MCP connection of another user refused"),
                }
            }
        }

        /// Nothing to prepare: the socket keeps accepting.
        pub(crate) fn rearm(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub(crate) async fn connect() -> Result<UnixStream, Unreachable> {
        connect_to(&path().map_err(Unreachable::Other)?).await
    }

    pub(crate) async fn connect_to(path: &Path) -> Result<UnixStream, Unreachable> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => Unreachable::Absent,
                _ => Unreachable::Other(e),
            })?;
        match stream.peer_cred() {
            Ok(peer) if peer.uid() == user() => Ok(stream),
            Ok(_) => Err(Unreachable::NotOurs),
            Err(e) => Err(Unreachable::Other(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[cfg(windows)]
    fn test_name(what: &str) -> String {
        format!(
            r"\\.\pipe\candeo-mcp-test-{what}-{}-{}",
            std::process::id(),
            testing::user_sid()
        )
    }

    /// The pipe's DACL holds one entry, allowing the signed-in user: no other
    /// account, no group, no AppContainer.
    #[cfg(windows)]
    #[tokio::test]
    async fn the_pipe_grants_the_signed_in_user_alone() {
        use std::os::windows::io::AsRawHandle;
        let name = test_name("dacl");
        let mut listener = Listener::bind_at(&name).unwrap();
        let client = tokio::spawn({
            let name = name.clone();
            async move { connect_to(&name) }
        });
        let server = listener.accept().await.unwrap();
        assert_eq!(
            testing::dacl(server.as_raw_handle()),
            vec![(true, testing::user_sid())]
        );
        assert!(client.await.unwrap().is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn the_name_and_the_descriptor_carry_the_users_sid() {
        let sid = "S-1-5-21-1111111111-2222222222-3333333333-1001";
        assert_eq!(name_for(sid), format!(r"\\.\pipe\candeo-mcp-{sid}"));
        assert_eq!(sddl(sid), format!("D:P(A;;GA;;;{sid})"));
        assert!(name().unwrap().ends_with(&testing::user_sid()));
    }

    /// The name is held: a second listener does not join the first, it is told
    /// the name is taken.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_second_listener_finds_the_name_taken() {
        let name = test_name("taken");
        let _first = Listener::bind_at(&name).unwrap();
        assert!(matches!(Listener::bind_at(&name), Err(BindError::Taken)));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn no_pipe_reads_as_absent() {
        assert!(matches!(
            connect_to(&test_name("absent")),
            Err(Unreachable::Absent)
        ));
    }

    /// Bytes go both ways, and a pipe this user created is this user's.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_client_of_the_same_user_talks_to_the_server() {
        let name = test_name("talk");
        let mut listener = Listener::bind_at(&name).unwrap();
        let server = tokio::spawn(async move {
            let mut server = listener.accept().await.unwrap();
            listener.rearm().unwrap();
            let mut line = [0u8; 5];
            server.read_exact(&mut line).await.unwrap();
            server.write_all(b"pong\n").await.unwrap();
            line
        });
        let mut client = connect_to(&name).unwrap();
        client.write_all(b"ping\n").await.unwrap();
        let mut answer = [0u8; 5];
        client.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"pong\n");
        assert_eq!(&server.await.unwrap(), b"ping\n");
    }

    /// Four connections are served; a fifth connects, and waits.
    #[cfg(windows)]
    #[tokio::test]
    async fn every_connection_has_its_instance() {
        let name = test_name("many");
        let mut listener = Listener::bind_at(&name).unwrap();
        let mut clients = Vec::new();
        let accepting = tokio::spawn(async move {
            let mut served = Vec::new();
            for _ in 0..MAX_CONNECTIONS {
                served.push(listener.accept().await.unwrap());
                listener.rearm().unwrap();
            }
            (listener, served)
        });
        for _ in 0..MAX_CONNECTIONS {
            loop {
                match connect_to(&name) {
                    Ok(client) => break clients.push(client),
                    Err(Unreachable::Busy) | Err(Unreachable::Absent) => {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await
                    }
                    Err(e) => panic!("{e:?}"),
                }
            }
        }
        let (_listener, served) = accepting.await.unwrap();
        assert_eq!(served.len(), MAX_CONNECTIONS);
        // The instance created after the fourth takes a fifth client, which the
        // server holds until a slot frees.
        assert!(connect_to(&name).is_ok());
        // And a sixth finds every instance busy.
        assert!(matches!(connect_to(&name), Err(Unreachable::Busy)));
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_lives_in_the_runtime_folder_or_the_data_folder() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let some = |s: &str| Some(OsString::from(s));
        assert_eq!(
            socket_path(some("/run/user/1000"), None, some("/home/someone")),
            Some(PathBuf::from("/run/user/1000/candeo/mcp.sock"))
        );
        // Relative values are ignored, as the XDG specification says.
        let fallback = socket_path(some("run"), some("data"), some("/home/someone")).unwrap();
        assert!(
            fallback.ends_with("com.o2csi.candeo/mcp/mcp.sock"),
            "{fallback:?}"
        );
        assert!(fallback.starts_with("/home/someone"), "{fallback:?}");
        assert_eq!(socket_path(None, None, None), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_socket_is_the_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candeo").join("mcp.sock");
        let mut listener = Listener::bind_at(&path).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);

        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let mut line = [0u8; 5];
            stream.read_exact(&mut line).await.unwrap();
            stream.write_all(b"pong\n").await.unwrap();
            (listener, line)
        });
        let mut client = connect_to(&path).await.unwrap();
        client.write_all(b"ping\n").await.unwrap();
        let mut answer = [0u8; 5];
        client.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"pong\n");
        let (listener, line) = server.await.unwrap();
        assert_eq!(&line, b"ping\n");

        // Held while it listens; gone once it stops.
        assert!(matches!(Listener::bind_at(&path), Err(BindError::Taken)));
        drop(listener);
        assert!(matches!(connect_to(&path).await, Err(Unreachable::Absent)));
    }

    /// The identifier the socket's fallback folder is named after is the one
    /// Tauri names the data folder after.
    #[cfg(unix)]
    #[test]
    fn the_identifier_is_tauris() {
        let config = include_str!("../../tauri.conf.json");
        assert!(config.contains(&format!("\"identifier\": \"{IDENTIFIER}\"")));
    }
}
