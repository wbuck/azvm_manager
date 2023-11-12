use azure_identity::AzureCliCredential;
use azure_core::{RetryOptions, ExponentialRetryOptions, auth::TokenCredential, StatusCode};
use clap::{Parser, Subcommand, Args};
use futures_util::{StreamExt, TryStreamExt};
use std::sync::Arc;
use azure_core::headers::HeaderName;
use log::debug;
use store::Store;
use azure_mgmt_resources::{Client as ResourceClient, models::ResourceGroup};
use azure_mgmt_subscription::{Client as SubscriptionClient, models::Subscription};
use azure_mgmt_recoveryservicesbackup::{Client as BackupClient};
use azure_mgmt_recoveryservicesbackup::models::{
    AzureIaaSvmProtectedItem,
    OperationStatus,
    operation_status::Status as OpStatus,
    ProtectedItem,
    ProtectedItemResource,
    ProtectedItemUnion,
    Resource as RequestResource
};
use azure_mgmt_recoveryservicesbackup::models::operation_status::Status;
use azure_mgmt_recoveryservicesbackup::models::protected_item::{BackupManagementType, WorkloadType};
use tokio::time::{sleep_until, Duration, Instant};
use dsp::{display_rg, display_sub, display_vm, Output};
use spinoff::{Spinner, spinners, Color};
use url::Url;

use crate::vm_client::{VmClient, VmCommand};

mod error;
mod vm_client;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sets the default Azure subscription ID.
    #[arg(long)]
    set_sub: Option<String>,

    /// Sets the default Azure resource group.
    #[arg(long)]
    set_rg: Option<String>,

    /// Sets the vaults default Azure resource group.
    #[arg(long)]
    set_vault_rg: Option<String>,

    /// Sets the default vault name.
    #[arg(long)]
    set_vault: Option<String>,

    #[command(subcommand)]
    command: Option<Cmd>
}

#[derive(Subcommand, Debug)]
enum Cmd { 
    /// A set of commands for Azure subscriptions.
    Sub(SubArgs),
    Rg(RgArgs),
    Vm(VmArgs),
    Recovery(RecoveryArgs)
}

#[derive(Args, Debug)]
struct RecoveryArgs {
    #[command(subcommand)]
    command: RecoveryCmd
}

#[derive(Subcommand, Debug)]
enum RecoveryCmd {
    Backup {
        #[arg(long)]
        vault_name: Option<String>,

        #[arg(long)]
        vault_group: Option<String>,

        #[arg(short, long)]
        group: Option<String>,

        #[arg(short, long)]
        sub_id: Option<String>,

        #[arg(short, long, num_args = 1.., value_delimiter = ',')]
        names: Option<Vec<String>>,
    }
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
        debug!("Setting default subscription to: {sub_id}");
        store.set_subscription_id(sub_id);
    }

    if let Some(rg) = cli.set_rg.as_deref() {
        debug!("Setting default resource group to: {rg}");
        store.set_resource_group(rg); 
    }

    if let Some(rg) = cli.set_vault_rg.as_deref() {
        debug!("Setting default vault resource group to: {rg}");
        store.set_vault_resource_group(rg);
    }

    if let Some(name) = cli.set_vault.as_deref() {
        debug!("Setting default vault name to: {name}");
        store.set_vault_name(name);
    }

    if cli.set_sub.is_some() ||
        cli.set_rg.is_some() ||
        cli.set_vault_rg.is_some() ||
        cli.set_vault.is_some()
    {
        debug!("Saving store file");

        let mut spinner = Spinner::new(
            spinners::Dots,
            format!("Saving configuration..."),
            Color::Blue
        );

        store.save().await.expect("Failed to save store file");

        spinner.clear();
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

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading subscription..."),
                Color::Blue
            );

            let sub = client.subscriptions_client()
                .get(sub_id)
                .await?;

            spinner.clear();
            display_sub(Output::Single(&sub));
        },
        SubCmd::List => {

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading subscriptions..."),
                Color::Blue
            );

            let subs: Vec<Subscription> = client.subscriptions_client()
                .list()
                .into_stream()
                .try_collect::<Vec<_>>()
                .await?
                .into_iter()
                .flat_map(|subs| subs.value)
                .collect();

            spinner.clear();
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

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading resource group..."),
                Color::Blue
            );

            let group = client.resource_groups_client()
                .get(group_name, sub_id)
                .await?;

            spinner.clear();
            display_rg(Output::Single(&group));
        },
        RgCmd::List { sub_id } => {
            let sub_id = match sub_id.as_deref() {
                Some(id) => id,
                None => store.get_subscription_id().ok_or(error::AppError::NoSub)?
            };

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading resource groups..."),
                Color::Blue
            );

            let groups: Vec<ResourceGroup> = client.resource_groups_client()
                .list(sub_id)
                .into_stream()
                .try_collect::<Vec<_>>()
                .await?
                .into_iter()
                .flat_map(|groups| groups.value)
                .collect();

            spinner.clear();
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

        spinner.update_text(format!("{prefix} {completed}/{total} virtual machines..."));

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
    spinner.clear();

    let vms = client.list_vms_with_instance_view(
        group_name,
        subscription_id
    ).await?;

    display_vm(Output::Multiple(&vms));

    Ok(())
}

fn get_opt<'a, F>(opt: &'a Option<String>, f: F) -> Result<&'a str, error::AppError>
    where
        F: FnOnce() -> Result<&'a str, error::AppError>
{
    match opt.as_deref() {
        Some(id) => Ok(id),
        None => f()
    }
}

async fn process_vm_cmd(args: VmArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    let client = VmClient::new(creds);

    match args.command {
        VmCmd::Get { name, group, sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading virtual machine..."),
                Color::Blue
            );

            let vm = client.get_vm_with_instance_view(
                name.as_str(),
                group_name,
                subscription_id
            ).await?;

            spinner.clear();

            println!("{vm:#?}");

            display_vm(Output::Single(&vm));
        },
        VmCmd::List { group, sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading virtual machines..."),
                Color::Blue
            );

            let vms = client.list_vms_with_instance_view(
                group_name,
                subscription_id
            ).await?;

            spinner.clear();
            display_vm(Output::Multiple(&vms));
        },
        VmCmd::ListAll { sub_id } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Loading virtual machines..."),
                Color::Blue
            );

            let vms = client.list_all_vms(subscription_id).await?;

            spinner.clear();
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

async fn process_recovery_cmd(args: RecoveryArgs, store: &Store, creds: Arc<dyn TokenCredential>) -> Result<(), Box<dyn std::error::Error>> {
    let client = BackupClient::builder(creds.clone())
        .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
        .build();

    match args.command {
        RecoveryCmd::Backup { vault_name, vault_group, group, sub_id, names } => {
            let subscription_id = get_opt(&sub_id, || store.get_subscription_id()
                .ok_or(error::AppError::NoSub))?;

            let group_name = get_opt(&group, || store.get_resource_group()
                .ok_or(error::AppError::NoRg))?;

            let vault_name = get_opt(&vault_name, || store.get_vault_name()
                .ok_or(error::AppError::NoVault))?;

            let vault_group = get_opt(&vault_group, || store.get_vault_resource_group().or_else(|| Some(group_name))
                .ok_or(error::AppError::NoRg))?;

            let mut spinner = Spinner::new(
                spinners::Dots,
                format!("Refreshing recovery services vault..."),
                Color::Blue
            );

            let response = client.protection_containers_client().refresh(
                vault_name,
                vault_group,
                subscription_id,
                "Azure"
            ).send().await?;

            let location = response
                .as_ref()
                .headers()
                .get_optional_str(&HeaderName::from_static("location"))
                .ok_or(error::AppError::MissingLocationHeader)?;

            let location = Url::parse(location)?;
            let operation_id = location
                .path_segments()
                .expect("Invalid location header")
                .last()
                .unwrap();

            spinner.update_text("Waiting for completion of refresh...");

            loop {
                let response = client.protection_container_refresh_operation_results_client().get(
                    vault_name,
                    vault_group,
                    subscription_id,
                    "Azure",
                    operation_id
                ).send().await?;

                if response.as_ref().status().eq(&StatusCode::NoContent) {
                    break;
                }
            }

            spinner.update_text("Getting list of virtual machines..");

            let vm_client = VmClient::new(creds);

            let vm_names = names.unwrap_or_else(|| Vec::new());
            let vms = vm_client.list_vms(group_name, subscription_id).await?;


            let values = vms
                .into_iter()
                .filter_map(|vm| {
                    if !vm_names.is_empty() && !vm_names.iter().any(|name| Some(name) == vm.resource.name.as_ref()) {
                        return None;
                    }
                    match (vm.resource.name.as_ref(), vm.resource.id.as_ref()) {
                        (Some(name), Some(id)) => {
                            let container_name = format!("iaasvmcontainer;iaasvmcontainerv2;{group_name};{name}");
                            let protected_item_name = format!("vm;iaasvmcontainerv2;{group_name};{name}");
                            Some((container_name, protected_item_name, id.clone(), vm.resource))
                        },
                        _ => None
                    }
                })
                .collect::<Vec<_>>();

            let mut items = Vec::new();
            let total = values.len();
            let mut count = 0;

            spinner.update_text(format!("Protected {count}/{total} virtual machines"));

            for (container_name, protected_item_name, id, resource) in values {
                let resource = RequestResource {
                    id: resource.id,
                    name: resource.name,
                    location: Some(resource.location),
                    tags: resource.tags,
                    e_tag: None,
                    type_: resource.type_
                };

                let item = ProtectedItem {
                    backup_management_type: Some(BackupManagementType::AzureIaasVm),
                    workload_type: Some(WorkloadType::Vm),
                    container_name: Some(container_name.clone()),
                    source_resource_id: Some(id.clone()),
                    policy_id: Some(format!("subscriptions/{subscription_id}/resourceGroups/{vault_name}/providers/microsoft.recoveryservices/vaults/{vault_name}/backupPolicies/DefaultPolicy")),
                    last_recovery_point: None,
                    backup_set_name: None,
                    create_mode: None,
                    deferred_delete_time_in_utc: None,
                    is_scheduled_for_deferred_delete: None,
                    deferred_delete_time_remaining: None,
                    is_deferred_delete_schedule_upcoming: None,
                    is_rehydrate: None,
                    resource_guard_operation_requests: vec![],
                    is_archive_enabled: None,
                    policy_name: Some(String::from("DefaultPolicy")),
                    soft_delete_retention_period_in_days: None,
                };

                let properties = AzureIaaSvmProtectedItem {
                    protected_item: item,
                    friendly_name: None,
                    virtual_machine_id: None,
                    protection_status: None,
                    protection_state: None,
                    health_status: None,
                    health_details: vec![],
                    kpis_healths: None,
                    last_backup_status: None,
                    last_backup_time: None,
                    protected_item_data_id: None,
                    extended_info: None,
                    extended_properties: None,
                };

                let body = ProtectedItemResource {
                    resource,
                    properties: Some(ProtectedItemUnion::AzureIaaSvmProtectedItem(properties))
                };
                let response = client.protected_items_client().create_or_update(
                    vault_name,
                    vault_group,
                    subscription_id, "Azure",
                    container_name.as_str(),
                    protected_item_name.as_str(),
                    body
                ).send().await?;


                let location = response
                    .as_ref()
                    .headers()
                    .get_optional_str(&HeaderName::from_static("location"))
                    .ok_or(error::AppError::MissingLocationHeader)?;

                let location = Url::parse(location)?;
                let operation_id = location
                    .path_segments()
                    .expect("Invalid location header")
                    .last()
                    .unwrap();

                let retry_secs: u64 = response
                    .as_ref()
                    .headers()
                    .get_optional_str(&HeaderName::from_static("retry-after"))
                    .unwrap()
                    .parse()?;

                sleep_until(Instant::now() + Duration::from_secs(retry_secs)).await;


                loop {
                    let status = client.protected_item_operation_statuses_client().get(
                        vault_name,
                        vault_group,
                        subscription_id,
                        "Azure",
                        container_name.as_str(),
                        protected_item_name.as_str(),
                        operation_id
                    ).await?;

                    match status.status {
                        Some(OpStatus::Succeeded) => {
                            println!("Succeeded");

                            let item = client.protected_item_operation_results_client().get(
                                vault_name,
                                vault_group,
                                subscription_id,
                                "Azure",
                                container_name.as_str(),
                                protected_item_name.as_str(),
                                operation_id
                            ).await?;

                            items.push(item);

                            count += 1;
                            spinner.update_text(format!("Protected {count}/{total} virtual machines"));

                            break;
                        },
                        Some(OpStatus::Failed) => {
                            println!("Failed");
                            break;
                        },
                        Some(OpStatus::InProgress) => {
                            println!("In progress");
                            continue;
                        },
                        Some(OpStatus::Invalid) => {
                            println!("Invalid");
                            break;
                        },
                        Some(OpStatus::Canceled) => {
                            println!("Cancelled");
                            break;
                        },
                        Some(OpStatus::UnknownValue(value)) => {
                            println!("Unknown value: {value}");
                            break;
                        }
                        None => continue
                    }
                }

            }

            spinner.stop_with_message("All virtual machines protected");
            println!("{items:#?}");


            // let mut page = client.backup_protectable_items_client()
            //     .list(vault_name, vault_group, subscription_id)
            //     .filter("backupManagementType eq 'AzureIaasVM'")
            //     .into_stream();
            //
            // while let Some(vms) = page.next().await {
            //     let vms = vms?;
            //     for vm in vms.value.iter() {
            //         println!("{vm:#?}");
            //     }
            // }
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
        Some(Cmd::Recovery(args)) => {
            process_recovery_cmd(args, &store, creds).await?;
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
