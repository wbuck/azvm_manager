use azure_identity::AzureCliCredential;
use azure_core::{RetryOptions, ExponentialRetryOptions, auth::TokenCredential};
use clap::{Parser, Subcommand, Args};
use futures_util::TryStreamExt;
use std::sync::Arc;
use log::debug;
use store::Store;
use azure_mgmt_resources::{Client as ResourceClient, models::ResourceGroup};
use azure_mgmt_subscription::{Client as SubscriptionClient, models::Subscription};
use tokio::time::{sleep_until, Duration, Instant};
use dsp::{display_rg, display_sub, display_vm, Output};
use spinoff::{Spinner, spinners, Color};

use crate::vm_client::{VmClient, VmCommand};

mod error;
mod vm_client;

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
    Rg(RgArgs),
    Vm(VmArgs)
}

#[derive(Args, Debug)]
struct VmArgs {
    #[command(subcommand)]
    command: VmCmd
}

#[derive(Subcommand, Debug)]
enum VmCmd {
    Get {
        #[arg(short, long)]
        name: String,

        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>
    },
    List {
        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>
    },
    ListAll {
        #[arg(short, long)]
        sub_id: Option<String>
    },
    Start {
        #[arg(short, long, num_args = 1.., value_delimiter = ',')]
        names: Option<Vec<String>>,

        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>
    },
    Stop {
        #[arg(short, long, num_args = 1, value_delimiter = ',')]
        names: Option<Vec<String>>,

        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>
    }
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

async fn process_sub_cmd(args: SubArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {

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

            display_sub(Output::Single(&sub));
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

            display_sub(Output::Multiple(&subs));
        }
    }
    Ok(())
}

async fn process_rg_cmd(args: RgArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
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

            display_rg(Output::Single(&group));
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

            display_rg(Output::Multiple(&groups));
        }
    }

    Ok(())
}

async fn send_vm_command(client: &VmClient, vm_names: Option<Vec<String>>, group_name: &str, subscription_id: &str, command: VmCommand) -> Result<(), Box<dyn std::error::Error>> {
    let mut vm_names = match vm_names {
        Some(vm_names) => vm_names,
        None => client.list_vm_names(group_name, subscription_id).await?
    };

    client.command(vm_names.iter(), group_name, subscription_id, command).await?;

    let total = vm_names.len();
    let mut completed = 0;

    let (prefix, target_state) = match command {
        VmCommand::Start => ("Started", "VM running"),
        VmCommand::Stop => ("Stopped", "VM deallocated")
    };

    let mut spinner = Spinner::new(
        spinners::Dots,
        format!("{prefix} 0/{total} virtual machines..."),
        Color::Blue
    );

    loop {

        let done = client
            .is_complete(vm_names.iter(), group_name, subscription_id, target_state)
            .await?;

        completed += done.len();

        spinner.update(
            spinners::Dots,
            format!("{prefix} {completed}/{total} virtual machines..."),
            Color::Blue
        );

        let temp: Vec<String> = done.iter().map(|s| (*s).clone()).collect();
        for name in temp.iter() {
            if let Some(pos) = vm_names.iter().position(|n| n == name) {
                vm_names.remove(pos);
            }
        }

        if vm_names.is_empty() {
            break;
        }
        sleep_until(Instant::now() + Duration::from_secs(2)).await;
    }
    spinner.stop();

    let vms = client.list_vms_with_instance_view(
        group_name,
        subscription_id
    ).await?;

    display_vm(Output::Multiple(&vms));

    Ok(())
}

async fn process_vm_cmd(args: VmArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    let client = VmClient::new(creds);

    fn get_opt<'a, F>(opt: &'a Option<String>, f: F) -> Result<&'a str, error::AppError>
    where
        F: FnOnce() -> Result<&'a str, error::AppError>
    {
        match opt.as_deref() {
            Some(id) => Ok(id),
            None => f()
        }
    }

    match args.command {
        VmCmd::Get { name, group, sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            let vm = client.get_vm_with_instance_view(
                name.as_str(),
                group_name,
                subscription_id
            ).await?;

            display_vm(Output::Single(&vm));
        },
        VmCmd::List { group, sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            let vms = client.list_vms_with_instance_view(
                group_name,
                subscription_id
            ).await?;

            display_vm(Output::Multiple(&vms));
        },
        VmCmd::ListAll { sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let vms = client.list_all_vms(subscription_id).await?;
            display_vm(Output::Multiple(&vms));
        },
        VmCmd::Start { names, group, sub_id } => {

            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            send_vm_command(
                &client,
                names,
                group_name,
                subscription_id,
                VmCommand::Start
            ).await?;
        },
        VmCmd::Stop { names, group, sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            send_vm_command(
                &client,
                names,
                group_name,
                subscription_id,
                VmCommand::Stop
            ).await?;
        }
    }
    Ok(())
}

async fn process_cmds(cli: Cli, store: &mut Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Some(Cmd::Sub(args)) => {
            process_sub_cmd(args, &store, creds).await?;
        },
        Some(Cmd::Rg(args)) => {
            process_rg_cmd(args, &store, creds).await?;
        },
        Some(Cmd::Vm(args)) => {
            process_vm_cmd(args, &store, creds).await?;
        },
        None => {
            println!("No command specified");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn config() {
    colored::control::set_virtual_terminal(true).unwrap();
}

#[cfg(not(windows))]
fn config() {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    config();

    let cli = Cli::parse();

    let mut store = Store::get_or_create().await?;
    handle_globals(&cli, &mut store).await?;

    if cli.command.is_some() {
        debug!("Creating Azure credentials");
        let creds = Arc::new(AzureCliCredential::new());
        process_cmds(cli, &mut store, creds).await?;
    }

    Ok(())
}
