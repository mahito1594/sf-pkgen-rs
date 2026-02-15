use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Salesforce package.xml generator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Generate a package.xml interactively
    Generate(GenerateArgs),
}

#[derive(Debug, clap::Args)]
pub struct GenerateArgs {
    /// Target org alias or username
    #[arg(short = 'o', long = "target-org")]
    pub target_org: Option<String>,

    /// API version (e.g. "62.0")
    #[arg(short = 'a', long = "api-version")]
    pub api_version: Option<String>,

    /// Output file path
    #[arg(short = 'f', long = "output-file")]
    pub output_file: Option<PathBuf>,

    /// Run in non-interactive mode (requires --all or --types)
    #[arg(long = "non-interactive")]
    pub non_interactive: bool,

    /// Include all metadata types and components (non-interactive only)
    #[arg(long, conflicts_with = "types")]
    pub all: bool,

    /// Comma-separated list of metadata types (non-interactive only)
    #[arg(long, value_delimiter = ',')]
    pub types: Option<Vec<String>>,
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

    #[test]
    fn non_interactive_with_all_parses() {
        let cli = Cli::try_parse_from([
            "sf-pkgen",
            "generate",
            "--non-interactive",
            "--all",
            "-f",
            "package.xml",
        ])
        .unwrap();
        let Commands::Generate(args) = cli.command;
        assert!(args.non_interactive);
        assert!(args.all);
        assert!(args.types.is_none());
    }

    #[test]
    fn non_interactive_with_types_parses() {
        let cli = Cli::try_parse_from([
            "sf-pkgen",
            "generate",
            "--non-interactive",
            "--types",
            "ApexClass,Report",
            "-f",
            "package.xml",
        ])
        .unwrap();
        let Commands::Generate(args) = cli.command;
        assert!(args.non_interactive);
        assert!(!args.all);
        assert_eq!(
            args.types,
            Some(vec!["ApexClass".to_string(), "Report".to_string()])
        );
    }

    #[test]
    fn types_single_value_parses() {
        let cli = Cli::try_parse_from(["sf-pkgen", "generate", "--types", "ApexClass"]).unwrap();
        let Commands::Generate(args) = cli.command;
        assert_eq!(args.types, Some(vec!["ApexClass".to_string()]));
    }

    #[test]
    fn all_and_types_conflict() {
        let result = Cli::try_parse_from(["sf-pkgen", "generate", "--all", "--types", "ApexClass"]);
        assert!(result.is_err());
    }
}
