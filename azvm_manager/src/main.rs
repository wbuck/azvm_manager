use azure_identity::AzureCliCredential;
use azure_core::{RetryOptions, ExponentialRetryOptions, auth::{TokenResponse, TokenCredential}};
use clap::{Parser, Subcommand, Args};
use futures::stream::StreamExt;
use futures_util::TryStreamExt;
use std::sync::Arc;
use log::debug;
use store::Store;
use azure_mgmt_compute::Client as ComputeClient;
use azure_mgmt_resources::{Client as ResourceClient, models::ResourceGroup};
use azure_mgmt_subscription::{Client as SubscriptionClient, models::Subscription};
use dsp;

mod error;
use error::AppError;



#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sets the Azure subscription ID.
    #[arg(long)]
    set_sub: Option<String>,

    /// Set the Azure resource group.
    #[arg(long)]
    set_rg: Option<String>,

    #[command(subcommand)]
    command: Option<Cmd>
}

#[derive(Subcommand, Debug)]
enum Cmd { 
    /// A set of commands for Azure subscriptions.
    Sub(SubArgs),
    Rg(RgArgs)
}

#[derive(Args, Debug)]
struct RgArgs {
    #[command(subcommand)]
    command: RgCmd
}

#[derive(Subcommand, Debug)]
enum RgCmd {
    Get {
        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>
    },
    List {
        #[arg(short, long)]
        sub_id: Option<String>
    }
}

#[derive(Args, Debug)]
struct SubArgs {
    #[command(subcommand)]
    command: SubCmd
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    /// Displays information about about a subscription.
    Get {
        /// Displays information about the specified subscription, 
        /// else displays information about the currently selected subscription.
        #[arg(short, long)]
        id: Option<String>
    },
    /// Displays information about all subscriptions.
    List
}


#[derive(Subcommand, Debug)]
enum Commands2 {
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
    GetVms {
        #[arg(short, long)]
        sub: Option<String>,

        #[arg(short, long)]
        rg: Option<String>,

        #[arg(short, long)]
        verbose: bool
    }
}

async fn handle_globals(cli: &Cli, store: &mut Store) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(sub_id) = cli.set_sub.as_deref() {
        debug!("Setting subscription to: {sub_id}");
        store.set_subscription_id(sub_id);
    }

    if let Some(rg) = cli.set_rg.as_deref() {
        debug!("Setting resource group to: {rg}");
        store.set_resource_group(rg); 
    }    

    if cli.set_sub.is_some() || cli.set_rg.is_some() {
        debug!("Saving store file");
        store.save().await.expect("Failed to save store file");
    }
    Ok(())
}

async fn process_sub_cmd(args: &SubArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {

    let client = SubscriptionClient::builder(creds)
        .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
        .build();

    match &args.command {
        SubCmd::Get { id } => {
            let sub_id = match id.as_deref() {
                Some(id) => id,
                None => store.get_subscription_id().ok_or(error::AppError::NoSub)?,
            };

            let sub = client.subscriptions_client()
                .get(sub_id)
                .await?;

            dsp::display_sub(dsp::Output::Single(&sub));
        },
        SubCmd::List => {
            let subs: Vec<Subscription> = client.subscriptions_client()
                .list()
                .into_stream()
                .try_collect::<Vec<_>>()
                .await?
                .into_iter()
                .flat_map(|subs| subs.value)
                .collect();

            dsp::display_sub(dsp::Output::Multiple(&subs));
        }
    }
    Ok(())
}

async fn process_rg_cmd(args: &RgArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    let client = ResourceClient::builder(creds)
        .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
        .build();

    match &args.command {
        RgCmd::Get { group, sub_id } => {

            let sub_id = match sub_id.as_deref() {
                Some(id) => id,
                None => store.get_subscription_id().ok_or(error::AppError::NoSub)?
            };

            let group_name = match group.as_deref() {
                Some(name) => name,
                None => store.get_resource_group().ok_or(error::AppError::NoRg)?,
            };            

            let group = client.resource_groups_client()
                .get(group_name, sub_id)
                .await?;

            dsp::display_rg(dsp::Output::Single(&group));
        },
        RgCmd::List { sub_id } => {
            let sub_id = match sub_id.as_deref() {
                Some(id) => id,
                None => store.get_subscription_id().ok_or(error::AppError::NoSub)?
            };

            let groups: Vec<ResourceGroup> = client.resource_groups_client()
                .list(sub_id)
                .into_stream()
                .try_collect::<Vec<_>>()
                .await?
                .into_iter()
                .flat_map(|groups| groups.value)
                .collect();

            dsp::display_rg(dsp::Output::Multiple(&groups));
        }
    }

    Ok(())
}

async fn process_cmds(cli: &Cli, store: &mut Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    match &cli.command {
        Some(Cmd::Sub(args)) => {
            process_sub_cmd(args, &store, creds).await?;
        },
        Some(Cmd::Rg(args)) => {
            process_rg_cmd(args, &store, creds).await?;
        },
        None => {
            println!("No command specified");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut store = Store::get_or_create().await?;
    handle_globals(&cli, &mut store).await?;

    if cli.command.is_some() {
        debug!("Creating Azure credentials");
        let creds = Arc::new(AzureCliCredential::new());
        process_cmds(&cli, &mut store, creds).await?;  
    }
    
    

     

    // let sub_client = SubscriptionClient::builder(creds)
    //     .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
    //     .build();

    // let mut subs = sub_client
    //     .subscriptions_client()
    //     .list()
    //     .into_stream();

    // while let Some(subs) = subs.next().await {
    //     let subs = subs?;
    //     for sub in subs.value {
    //         println!("{:#?}", &sub);
    //     }
    // }
    

    

    //let mut groups = resource_client
    //    .resource_groups_client()
    //    .list(store.get_subscription_id().unwrap())
    //    .into_stream();
//
    //while let Some(groups) = groups.next().await {
    //    let groups = groups?;
    //    for group in groups.value {
    //        println!("{:#?}", &group);
    //    }
    //}

    //let compute_client = ComputeClient::builder(creds)
    //    .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
    //    .build();

   

    Ok(())
}
