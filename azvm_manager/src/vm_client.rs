use std::sync::Arc;
use azure_core::auth::TokenCredential;
use azure_core::{ExponentialRetryOptions, RetryOptions};
use azure_mgmt_compute::{Client, models::VirtualMachine};
use azure_mgmt_compute::models::{VirtualMachineInstanceView, VirtualMachineProperties};
use futures_util::TryStreamExt;

#[derive(Debug, Copy, Clone)]
pub enum VmCommand {
    Start,
    Stop
}

pub struct VmClient {
    client: Client
}

impl VmClient {
    pub fn new(creds: Arc<dyn TokenCredential>) -> Self {
        let client = Client::builder(creds)
            .retry(RetryOptions::exponential(ExponentialRetryOptions::default()))
            .build();

        Self {
            client
        }
    }

    pub async fn get_instance_view(&self, vm_name: &str, group_name: &str, subscription_id: &str) -> Result<VirtualMachineInstanceView, Box<dyn std::error::Error>> {
        let instance_view = self.client.virtual_machines_client()
            .instance_view(group_name, vm_name, subscription_id)
            .await?;

        Ok(instance_view)
    }

    pub async fn is_complete<I, T>(&self, vm_names: I, group_name: &str, subscription_id: &str, state: &str) -> Result<Vec<T>, Box<dyn std::error::Error>>
    where
        T: AsRef<str>,
        I: IntoIterator<Item = T>
    {
        let mut complete = Vec::<T>::new();
        for vm_name in vm_names.into_iter() {
            let view = self.get_instance_view(vm_name.as_ref(), group_name, subscription_id).await?;

            let status = view.statuses.iter()
                .filter(|s| s.code.as_deref().is_some_and(|c| c.contains("PowerState")))
                .map(|s| s.display_status.as_deref().unwrap_or_else(|| "Unknown"))
                .nth(0)
                .unwrap_or_else(|| "Unknown");

            if status.contains(state) {
                complete.push(vm_name);
            }
        }
        Ok(complete)
    }

    pub async fn get_vm(&self, vm_name: &str, group_name: &str, subscription_id: &str) -> Result<VirtualMachine, Box<dyn std::error::Error>> {
        let vm = self.client.virtual_machines_client()
            .get(group_name, vm_name, subscription_id)
            .await?;

        Ok(vm)
    }
    pub async fn get_vm_with_instance_view(&self, vm_name: &str, group_name: &str, subscription_id: &str) -> Result<VirtualMachine, Box<dyn std::error::Error>> {
        let mut vm = self.get_vm(vm_name, group_name, subscription_id)
            .await?;

        let instance_view = self
            .get_instance_view(vm_name, group_name, subscription_id)
            .await?;

        let properties = vm.properties.get_or_insert(VirtualMachineProperties::default());
        properties.instance_view = Some(instance_view);

        Ok(vm)
    }

    pub async fn list_vms(&self, group_name: &str, subscription_id: &str) -> Result<Vec<VirtualMachine>, Box<dyn std::error::Error>> {
        let vms: Vec<VirtualMachine> = self.client.virtual_machines_client()
            .list(group_name, subscription_id)
            .into_stream()
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .flat_map(|vms| vms.value)
            .collect();

        Ok(vms)
    }

    pub async fn list_vm_names(&self, group_name: &str, subscription_id: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let names: Vec<String> = self.list_vms(group_name, subscription_id)
            .await?
            .into_iter()
            .filter_map(|vm| vm.resource.name)
            .collect();

        Ok(names)
    }

    pub async fn list_all_vms(&self, subscription_id: &str) -> Result<Vec<VirtualMachine>, Box<dyn std::error::Error>> {
        let vms: Vec<VirtualMachine> = self.client.virtual_machines_client()
            .list_all(subscription_id)
            .status_only("true")
            .into_stream()
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .flat_map(|vms| vms.value)
            .collect();

        Ok(vms)
    }

    pub async fn list_vms_with_instance_view(&self, group_name: &str, subscription_id: &str) -> Result<Vec<VirtualMachine>, Box<dyn std::error::Error>> {
        let mut vms = self.list_vms(group_name, subscription_id).await?;

        for vm in vms.iter_mut().filter(|vm| vm.resource.name.is_some()) {
            let name = vm.resource.name.as_deref().unwrap();

            let instance_view = self
                .get_instance_view(name, group_name, subscription_id)
                .await?;

            let properties = vm.properties.get_or_insert(VirtualMachineProperties::default());
            properties.instance_view = Some(instance_view);
        }
        Ok(vms)
    }

    pub async fn command<I, T>(&self, vm_names: I, group_name: &str, subscription_id: &str, command: VmCommand) -> Result<(), Box<dyn std::error::Error>>
        where
            T: AsRef<str>,
            I: IntoIterator<Item = T>
    {
        match command {
            VmCommand::Start => self.start_vms(vm_names, group_name, subscription_id).await,
            VmCommand::Stop => self.stop_vms(vm_names, group_name, subscription_id).await
        }
    }

    pub async fn start_vms<I, T>(&self, vm_names: I, group_name: &str, subscription_id: &str) -> Result<(), Box<dyn std::error::Error>>
    where
        T: AsRef<str>,
        I: IntoIterator<Item = T>
    {
        for vm_name in vm_names.into_iter() {
            self.client.virtual_machines_client()
                .start(group_name, vm_name.as_ref(), subscription_id)
                .send()
                .await?;
        }
        Ok(())
    }

    pub async fn stop_vms<I, T>(&self, vm_names: I, group_name: &str, subscription_id: &str) -> Result<(), Box<dyn std::error::Error>>
        where
            T: AsRef<str>,
            I: IntoIterator<Item = T>
    {
        for vm_name in vm_names.into_iter() {
            self.client.virtual_machines_client()
                .deallocate(group_name, vm_name.as_ref(), subscription_id)
                .send()
                .await?;
        }
        Ok(())
    }
}

