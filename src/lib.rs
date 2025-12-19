pub mod codegen;
pub mod compile;
pub mod intermediate;

#[derive(Debug, Default, Clone, clap::Args)]
pub struct GlobalOptions {
    /// Add a safety limit (in bytes) to array parsing
    #[arg(long)]
    pub guard_array_size: Option<u64>,
}
