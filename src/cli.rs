use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Salesforce package.xml generator")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Generate a package.xml interactively
    Generate(GenerateArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct GenerateArgs {
    /// Target org alias or username
    #[arg(short = 'o', long = "target-org")]
    pub(crate) target_org: Option<String>,

    /// API version (e.g. "62.0")
    #[arg(short = 'a', long = "api-version")]
    pub(crate) api_version: Option<String>,

    /// Output file path
    #[arg(short = 'f', long = "output-file")]
    pub(crate) output_file: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_subcommand_is_error() {
        let result = Cli::try_parse_from(["sf-pkgen"]);
        assert!(result.is_err());
    }

    #[test]
    fn generate_subcommand_parses() {
        let cli = Cli::try_parse_from(["sf-pkgen", "generate"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert!(args.target_org.is_none());
        assert!(args.api_version.is_none());
        assert!(args.output_file.is_none());
    }

    #[test]
    fn target_org_short_and_long() {
        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "-o", "my-org"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.target_org.as_deref(), Some("my-org"));

        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "--target-org", "my-org"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.target_org.as_deref(), Some("my-org"));
    }

    #[test]
    fn api_version_short_and_long() {
        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "-a", "62.0"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.api_version.as_deref(), Some("62.0"));

        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "--api-version", "62.0"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.api_version.as_deref(), Some("62.0"));
    }

    #[test]
    fn output_file_short_and_long() {
        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "-f", "package.xml"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.output_file, Some(PathBuf::from("package.xml")));

        let cli = Cli::try_parse_from([
            "sf-pkgen",
            "generate",
            "--output-file",
            "manifest/package.xml",
        ])
        .unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(
            args.output_file,
            Some(PathBuf::from("manifest/package.xml"))
        );
    }

    #[test]
    fn all_options_together() {
        let cli = Cli::try_parse_from([
            "sf-pkgen",
            "generate",
            "-o",
            "my-org",
            "-a",
            "62.0",
            "-f",
            "package.xml",
        ])
        .unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.target_org.as_deref(), Some("my-org"));
        assert_eq!(args.api_version.as_deref(), Some("62.0"));
        assert_eq!(args.output_file, Some(PathBuf::from("package.xml")));
    }

    #[test]
    fn unknown_option_is_error() {
        let result = Cli::try_parse_from(["sf-pkgen", "generate", "--unknown"]);
        assert!(result.is_err());
    }
}
