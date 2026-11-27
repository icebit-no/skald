//! Where Skald keeps its files on each system. The rules take the environment as a value, so they are tested for
//! every system on any machine; `Env::from_process` reads this process's environment for the callers that run.

use std::path::{Path, PathBuf};

/// The environment variables the folders are derived from; `None` where a variable is unset or empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub home: Option<PathBuf>,
    /// Windows' `APPDATA` and `LOCALAPPDATA`.
    pub appdata: Option<PathBuf>,
    pub local_appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
}

impl Env {
    /// The environment of this process, read once.
    pub fn from_process() -> Self {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            home: var("HOME").or_else(|| var("USERPROFILE")),
            appdata: var("APPDATA"),
            local_appdata: var("LOCALAPPDATA"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            xdg_data_home: var("XDG_DATA_HOME"),
            xdg_state_home: var("XDG_STATE_HOME"),
            xdg_runtime_dir: var("XDG_RUNTIME_DIR"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    pub const CURRENT: Os = if cfg!(target_os = "macos") {
        Os::MacOs
    } else if cfg!(windows) {
        Os::Windows
    } else {
        Os::Linux
    };
}

/// Skald's files. On macOS everything sits in one folder under Application Support; Windows and Linux split
/// settings, data and state the way those systems expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    state_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl Paths {
    /// The folders for `os`, or `None` when the environment names no home folder to put them in.
    pub fn for_os(os: Os, env: &Env) -> Option<Self> {
        match os {
            Os::MacOs => {
                let base = env.home.as_ref()?.join("Library/Application Support/Skald");
                Some(Self::in_one_folder(&base))
            }
            Os::Windows => {
                let roaming = env
                    .appdata
                    .clone()
                    .or_else(|| env.home.as_ref().map(|h| h.join("AppData/Roaming")))?
                    .join("Skald");
                let local = env
                    .local_appdata
                    .clone()
                    .or_else(|| env.home.as_ref().map(|h| h.join("AppData/Local")))?
                    .join("Skald");
                Some(Self {
                    config_dir: roaming,
                    data_dir: local.clone(),
                    state_dir: local.clone(),
                    runtime_dir: local,
                })
            }
            Os::Linux => {
                let home = env.home.as_ref();
                let xdg = |set: &Option<PathBuf>, fallback: &str| {
                    set.clone()
                        .filter(|p| p.is_absolute())
                        .or_else(|| home.map(|h| h.join(fallback)))
                };
                let config_dir = xdg(&env.xdg_config_home, ".config")?.join("skald");
                let data_dir = xdg(&env.xdg_data_home, ".local/share")?.join("skald");
                let state_dir = xdg(&env.xdg_state_home, ".local/state")?.join("skald");
                let runtime_dir = env
                    .xdg_runtime_dir
                    .clone()
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join("skald"))
                    .unwrap_or_else(|| state_dir.clone());
                Some(Self {
                    config_dir,
                    data_dir,
                    state_dir,
                    runtime_dir,
                })
            }
        }
    }

    /// Every file in one folder, as on macOS; also what a folder given on the command line means.
    pub fn in_one_folder(base: &Path) -> Self {
        Self {
            config_dir: base.to_path_buf(),
            data_dir: base.to_path_buf(),
            state_dir: base.to_path_buf(),
            runtime_dir: base.to_path_buf(),
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// History and correction counts, written after a take; never the configuration (D-19).
    pub fn state_file(&self) -> PathBuf {
        self.state_dir.join("state.toml")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// The local socket the command line talks to the running Skald over. Unix sockets take paths of at most
    /// 104 bytes on macOS, so this stays short under the folders above.
    pub fn socket(&self) -> PathBuf {
        self.runtime_dir.join("skald.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(home: &str) -> Env {
        Env {
            home: Some(PathBuf::from(home)),
            ..Env::default()
        }
    }

    #[test]
    fn macos_keeps_everything_under_application_support() {
        let paths = Paths::for_os(Os::MacOs, &env("/Users/a")).unwrap();
        let base = Path::new("/Users/a/Library/Application Support/Skald");
        assert_eq!(paths.config_file(), base.join("config.toml"));
        assert_eq!(paths.state_file(), base.join("state.toml"));
        assert_eq!(paths.models_dir(), base.join("models"));
        assert_eq!(paths.socket(), base.join("skald.sock"));
    }

    #[test]
    fn macos_socket_path_fits_a_long_user_name() {
        let paths = Paths::for_os(Os::MacOs, &env("/Users/a-rather-long-user-name")).unwrap();
        assert!(paths.socket().as_os_str().len() <= 104);
    }

    #[test]
    fn windows_splits_roaming_settings_from_local_data() {
        let env = Env {
            appdata: Some(PathBuf::from(r"C:\Users\a\AppData\Roaming")),
            local_appdata: Some(PathBuf::from(r"C:\Users\a\AppData\Local")),
            ..Env::default()
        };
        let paths = Paths::for_os(Os::Windows, &env).unwrap();
        assert_eq!(
            paths.config_file(),
            Path::new(r"C:\Users\a\AppData\Roaming")
                .join("Skald")
                .join("config.toml")
        );
        assert_eq!(
            paths.models_dir(),
            Path::new(r"C:\Users\a\AppData\Local")
                .join("Skald")
                .join("models")
        );
    }

    #[test]
    fn linux_follows_xdg_and_falls_back_to_home() {
        let paths = Paths::for_os(Os::Linux, &env("/home/a")).unwrap();
        assert_eq!(
            paths.config_file(),
            Path::new("/home/a/.config/skald/config.toml")
        );
        assert_eq!(
            paths.models_dir(),
            Path::new("/home/a/.local/share/skald/models")
        );
        assert_eq!(
            paths.state_file(),
            Path::new("/home/a/.local/state/skald/state.toml")
        );
        assert_eq!(
            paths.socket(),
            Path::new("/home/a/.local/state/skald/skald.sock")
        );

        let with_xdg = Env {
            xdg_config_home: Some(PathBuf::from("/cfg")),
            xdg_runtime_dir: Some(PathBuf::from("/run/user/1000")),
            ..env("/home/a")
        };
        let paths = Paths::for_os(Os::Linux, &with_xdg).unwrap();
        assert_eq!(paths.config_file(), Path::new("/cfg/skald/config.toml"));
        assert_eq!(paths.socket(), Path::new("/run/user/1000/skald/skald.sock"));
    }

    #[test]
    fn linux_ignores_relative_xdg_paths() {
        let relative = Env {
            xdg_config_home: Some(PathBuf::from("relative")),
            ..env("/home/a")
        };
        let paths = Paths::for_os(Os::Linux, &relative).unwrap();
        assert_eq!(
            paths.config_file(),
            Path::new("/home/a/.config/skald/config.toml")
        );
    }

    #[test]
    fn no_home_means_no_folders() {
        assert_eq!(Paths::for_os(Os::MacOs, &Env::default()), None);
        assert_eq!(Paths::for_os(Os::Linux, &Env::default()), None);
        assert_eq!(Paths::for_os(Os::Windows, &Env::default()), None);
    }

    #[test]
    fn one_folder_holds_everything() {
        let paths = Paths::in_one_folder(Path::new("/tmp/skald-test"));
        assert_eq!(
            paths.config_file(),
            Path::new("/tmp/skald-test/config.toml")
        );
        assert_eq!(paths.socket(), Path::new("/tmp/skald-test/skald.sock"));
    }
}
