use std::path::PathBuf;

use clap::Parser;
use xshell::cmd;

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    match args.cmd {
        SubCmd::DockerPublish(c) => c.run(),
    }
}

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    cmd: SubCmd,
}

#[derive(clap::Parser, Debug)]
enum SubCmd {
    DockerPublish(CmdDockerPublish),
}

pub trait CliCommand {
    fn run(self) -> Result<(), anyhow::Error>;
}

#[derive(clap::Parser, Debug)]
struct CmdDockerPublish {
    tag: String,
}

impl CliCommand for CmdDockerPublish {
    fn run(self) -> Result<(), anyhow::Error> {
        let root = root_path();
        let tag = self.tag;

        let sh = xshell::Shell::new()?;
        sh.change_dir(&root);

        cmd!(sh, "docker build . --tag theduke/daprox:{tag}").run()?;
        cmd!(sh, "docker push theduke/daprox:{tag}").run()?;

        Ok(())
    }
}

fn root_path() -> PathBuf {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
