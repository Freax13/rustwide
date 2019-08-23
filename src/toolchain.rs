use crate::cmd::{Binary, Command, Runnable};
use crate::tools::{RUSTUP, RUSTUP_TOOLCHAIN_INSTALL_MASTER};
use crate::Workspace;
use failure::{bail, Error, ResultExt};
use log::info;
use std::borrow::Cow;

pub(crate) const MAIN_TOOLCHAIN_NAME: &str = "stable";

/// Representation of a Rust compiler toolchain.
///
/// The `Toolchain` enum represents a compiler toolchain, either downloaded from rustup or from the
/// [rust-lang/rust][rustc] repo's CI artifacts storage. and it provides the tool to install and use it.
///
/// [rustc]: https://github.com/rust-lang/rust
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum Toolchain {
    /// Toolchain available through rustup and distributed from
    /// [static.rust-lang.org](https://static.rust-lang.org).
    Dist {
        /// The name of the toolchain, which is the same you'd use with `rustup toolchain install
        /// <name>`.
        name: Cow<'static, str>,
    },
    /// CI artifact from the [rust-lang/rust] repo. Each merged PR has its own full build
    /// available for a while after it's been merged, identified by the merge commit sha. **There
    /// is no retention or stability guarantee for these builds**.
    ///
    /// [rust-lang/rust]: https://github.com/rust-lang/rust
    #[serde(rename = "ci")]
    CI {
        /// Hash of the merge commit of the PR you want to download.
        sha: Cow<'static, str>,
        /// Whether you want to download a standard or "alt" build. "alt" builds have extra
        /// compiler assertions enabled.
        alt: bool,
    },
}

impl Toolchain {
    pub(crate) const MAIN: Toolchain = Toolchain::Dist {
        name: Cow::Borrowed(MAIN_TOOLCHAIN_NAME),
    };

    /// Download and install the toolchain.
    pub fn install(&self, workspace: &Workspace) -> Result<(), Error> {
        match self {
            Self::Dist { name } => init_toolchain_from_dist(workspace, name)?,
            Self::CI { sha, alt } => init_toolchain_from_ci(workspace, *alt, sha)?,
        }

        Ok(())
    }

    /// Download and install a component for the toolchain.
    pub fn add_component(&self, workspace: &Workspace, name: &str) -> Result<(), Error> {
        self.add_rustup_thing(workspace, "component", name)
    }

    /// Download and install a target for the toolchain.
    pub fn add_target(&self, workspace: &Workspace, name: &str) -> Result<(), Error> {
        self.add_rustup_thing(workspace, "target", name)
    }

    fn add_rustup_thing(
        &self,
        workspace: &Workspace,
        thing: &str,
        name: &str,
    ) -> Result<(), Error> {
        if let Self::CI { .. } = self {
            bail!("installing {} on CI toolchains is not supported yet", thing);
        }
        let toolchain_name = self.rustup_name();
        info!(
            "installing {} {} for toolchain {}",
            thing, name, toolchain_name
        );

        Command::new(workspace, &RUSTUP)
            .args(&[thing, "add", "--toolchain", &toolchain_name, name])
            .run()
            .with_context(|_| {
                format!(
                    "unable to install {} {} for toolchain {} via rustup",
                    thing, name, toolchain_name,
                )
            })?;
        Ok(())
    }

    /// Remove the toolchain from the rustwide workspace, freeing up disk space.
    pub fn uninstall(&self, workspace: &Workspace) -> Result<(), Error> {
        let name = self.rustup_name();
        Command::new(workspace, &RUSTUP)
            .args(&["toolchain", "uninstall", &name])
            .run()
            .with_context(|_| format!("unable to uninstall toolchain {} via rustup", name))?;
        Ok(())
    }

    /// Return a runnable object configured to run `cargo` with this toolchain. This method is
    /// intended to be used with [`rustwide::cmd::Command`](cmd/struct.Command.html).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use rustwide::{WorkspaceBuilder, Toolchain, cmd::Command};
    /// # use std::error::Error;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// # let workspace = WorkspaceBuilder::new("".as_ref(), "").init()?;
    /// let toolchain = Toolchain::Dist { name: "beta".into() };
    /// Command::new(&workspace, toolchain.cargo())
    ///     .args(&["check"])
    ///     .run()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn cargo<'a>(&'a self) -> impl Runnable + 'a {
        struct CargoBin<'a>(&'a Toolchain);

        impl Runnable for CargoBin<'_> {
            fn name(&self) -> Binary {
                Binary::ManagedByRustwide("cargo".into())
            }

            fn prepare_command<'w, 'pl>(&self, cmd: Command<'w, 'pl>) -> Command<'w, 'pl> {
                cmd.args(&[format!("+{}", self.0.rustup_name())])
            }
        }

        CargoBin(self)
    }

    fn rustup_name(&self) -> String {
        match self {
            Self::Dist { name } => name.to_string(),
            Self::CI { sha, alt: false } => sha.to_string(),
            Self::CI { sha, alt: true } => format!("{}-alt", sha),
        }
    }
}

impl std::fmt::Display for Toolchain {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.rustup_name())
    }
}

fn init_toolchain_from_dist(workspace: &Workspace, toolchain: &str) -> Result<(), Error> {
    info!("installing toolchain {}", toolchain);
    Command::new(workspace, &RUSTUP)
        .args(&["toolchain", "install", toolchain])
        .run()
        .with_context(|_| format!("unable to install toolchain {} via rustup", toolchain))?;

    Ok(())
}

fn init_toolchain_from_ci(workspace: &Workspace, alt: bool, sha: &str) -> Result<(), Error> {
    if alt {
        info!("installing toolchain {}-alt", sha);
    } else {
        info!("installing toolchain {}", sha);
    }

    let mut args = vec![sha, "-c", "cargo"];
    if alt {
        args.push("--alt");
    }

    Command::new(workspace, &RUSTUP_TOOLCHAIN_INSTALL_MASTER)
        .args(&args)
        .run()
        .with_context(|_| {
            format!(
                "unable to install toolchain {} via rustup-toolchain-install-master",
                sha
            )
        })?;

    Ok(())
}
