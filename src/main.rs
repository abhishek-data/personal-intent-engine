use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "pie",
    about = "Personal Intent Engine - Intelligent AI middleware"
)]
struct Args {
    /// Text input to process
    #[arg(trailing_var_arg = true)]
    input: Vec<String>,

    /// Optimization mode
    #[arg(short, long, default_value = "balanced")]
    mode: String,

    /// LLM provider
    #[arg(short, long, default_value = "openai")]
    provider: String,

    /// Model name
    #[arg(long)]
    model: Option<String>,

    /// Enable voice input (requires microphone)
    #[arg(long)]
    voice: bool,

    /// Verbose output (show intent, optimized prompt, etc.)
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let input_text = if args.input.is_empty() {
        // Read from stdin
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer)?;
        buffer.trim().to_string()
    } else {
        args.input.join(" ")
    };

    if input_text.is_empty() {
        eprintln!("Usage: pie \"your text here\" or pipe via stdin");
        std::process::exit(1);
    }

    // Create PIE engine
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut engine = pie_engine::PieEngine::new().await?;

        if args.verbose {
            println!("[PIE] Input: {}", input_text);
            println!("[PIE] Mode: {}", args.mode);
            println!("[PIE] Provider: {}", args.provider);
            println!();
        }

        let result = engine.process(&input_text, &args.mode).await?;

        if args.verbose {
            println!("[PIE] Detected intent:");
            println!("  Objective: {}", result.intent.objective);
            println!("  Type: {:?}", result.intent.conversation_type);
            println!("  Confidence: {:?}", result.intent.confidence);
            println!("  Context: {:?}", result.intent.context);
            println!("  Constraints: {:?}", result.intent.constraints);
            println!();
            println!(
                "[PIE] Optimized prompt ({} chars):",
                result.optimized_prompt.len()
            );
            println!("{}", result.optimized_prompt);
            println!();
        }

        // Send to LLM
        let response = engine
            .send_to_llm(
                &result.optimized_prompt,
                &args.provider,
                args.model.as_deref(),
            )
            .await?;

        println!("{}", response);
        Ok(())
    })
}
