use std::{env, io, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgPaths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
}

impl XdgPaths {
    pub fn from_environment() -> io::Result<Self> {
        Self::resolve(
            env::var_os("HOME").map(PathBuf::from),
            [
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
                "XDG_STATE_HOME",
            ]
            .map(|name| env::var_os(name).map(PathBuf::from)),
        )
    }

    fn resolve(home: Option<PathBuf>, overrides: [Option<PathBuf>; 4]) -> io::Result<Self> {
        let home = home.filter(|path| path.is_absolute());
        let resolve_path = |override_path: Option<PathBuf>, suffix: &str| {
            override_path
                .filter(|path| path.is_absolute())
                .or_else(|| home.as_ref().map(|path| path.join(suffix)))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "no absolute HOME or XDG directory override",
                    )
                })
        };
        let [config, data, cache, state] = overrides;
        Ok(Self {
            config: resolve_path(config, ".config")?,
            data: resolve_path(data, ".local/share")?,
            cache: resolve_path(cache, ".cache")?,
            state: resolve_path(state, ".local/state")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_overrides_work_without_home() {
        let paths = XdgPaths::resolve(
            None,
            [
                Some("/config"),
                Some("/data"),
                Some("/cache"),
                Some("/state"),
            ]
            .map(|value| value.map(PathBuf::from)),
        )
        .unwrap();
        assert_eq!(paths.config, PathBuf::from("/config"));
        assert_eq!(paths.data, PathBuf::from("/data"));
        assert_eq!(paths.cache, PathBuf::from("/cache"));
        assert_eq!(paths.state, PathBuf::from("/state"));
    }

    #[test]
    fn empty_relative_and_missing_overrides_use_home_defaults() {
        let paths = XdgPaths::resolve(
            Some(PathBuf::from("/home/test")),
            [
                Some(PathBuf::new()),
                Some(PathBuf::from("relative")),
                None,
                None,
            ],
        )
        .unwrap();
        assert_eq!(paths.config, PathBuf::from("/home/test/.config"));
        assert_eq!(paths.data, PathBuf::from("/home/test/.local/share"));
        assert_eq!(paths.cache, PathBuf::from("/home/test/.cache"));
        assert_eq!(paths.state, PathBuf::from("/home/test/.local/state"));
    }

    #[test]
    fn missing_or_relative_home_cannot_create_relative_defaults() {
        for home in [None, Some(PathBuf::from("relative"))] {
            let error = XdgPaths::resolve(home, [None, None, None, None]).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn mixed_overrides_without_home_require_every_directory_to_be_absolute() {
        for missing_index in 0..4 {
            for invalid_override in [None, Some(PathBuf::new()), Some(PathBuf::from("relative"))] {
                let mut overrides =
                    ["/config", "/data", "/cache", "/state"].map(|path| Some(PathBuf::from(path)));
                overrides[missing_index] = invalid_override;
                let error = XdgPaths::resolve(None, overrides).unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::NotFound);
            }
        }
    }
}
