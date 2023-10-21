use clap::{Parser, Subcommand};


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sets the Azure subscription ID.
    #[arg(long)]
    set_sub_id: Option<String>,

    /// Set the Azure resource group.
    #[arg(long)]
    set_rg: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Displays information about the currently selected subscription.
    GetSub {
        #[arg(short, long)]
        verbose: bool,

        /// Displays information about the specified subscription, 
        /// else displays information about the currently selected subscription.
        #[arg(short, long)]
        id: Option<String>
    },
    /// Displays information about all subscriptions.
    GetSubs {
        #[arg(short, long)]
        verbose: bool
    },
    /// Displays information about all resource groups for the currently selected subscription.
    GetRgs,
    /// Displays information about a specific resource group.
    GetRg {
        /// Displays information about the specified resource group, 
        /// else displays information about the currently selected resource group.
        #[arg(short, long)]
        name: Option<String>
    },
    GetVms
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Hello, world!");
    Ok(())
}
