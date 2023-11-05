use tabled::{
    Table,
    Tabled,
    settings::{
        object::{Columns, Rows, Object},
        Color,
        Modify,
        CellOption
    },
    grid::{
        config::{ColoredConfig, Entity},
        records::{ExactRecords, Records, vec_records::{CellInfo, VecRecords}}
    }
};

use std::borrow::Cow;
use std::iter;
use azure_mgmt_compute::models::VirtualMachine;
use crate::{Output, get_style};

pub fn display_vm(out: Output<VirtualMachine>) {
    let mut table = match out {
        Output::Single(vm) => Table::new(iter::once(Row(vm))),
        Output::Multiple(vms) => Table::new(vms.iter().map(|vm| Row(vm)))
    };

    table
        .with(get_style())
        .with(Modify::new(Columns::last().not(Rows::first())).with(Colorization));

    println!("{table}");
}

struct Row<'a>(&'a VirtualMachine);

impl<'a> Tabled for Row<'a> {
    const LENGTH: usize = 6;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        let mut vec = vec![
            Cow::from(self.0.resource.name.as_deref().unwrap_or("")),
            Cow::from(self.0.resource.location.as_str())
        ];

        if let Some(ref properties) = self.0.properties {
            let os_info = properties.storage_profile.as_ref()
                .and_then(|profile| profile.image_reference.as_ref().and_then(|image| {
                    Some((
                        image.offer.as_deref().unwrap_or(""),
                        image.sku.as_deref().unwrap_or(""),
                        image.version.as_deref().unwrap_or("")
                    ))
                }));

            vec.push(Cow::from(os_info.and_then(|(os, _, _)| Some(os)).unwrap_or("")));
            vec.push(Cow::from(os_info.and_then(|(_, sku, _)| Some(sku)).unwrap_or("")));
            vec.push(Cow::from(os_info.and_then(|(_, _, version)| Some(version)).unwrap_or("")));

            let status = match &properties.instance_view {
                Some(view) => {
                    view.statuses.iter()
                        .filter(|s| s.code.as_deref().is_some_and(|c| c.contains("PowerState")))
                        .map(|s| s.display_status.as_deref().unwrap_or_else(|| "Unknown"))
                        .nth(0)
                        .unwrap_or_else(|| "Unknown")
                },
                None => "Unknown"
            };
            vec.push(Cow::from(status));
        }

        vec
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![
            Cow::from("Name"),
            Cow::from("Location"),
            Cow::from("OS"),
            Cow::from("SKU"),
            Cow::from("Version"),
            Cow::from("Status")
        ]
    }
}

#[derive(Clone)]
struct Colorization;

impl CellOption<VecRecords<CellInfo<String>>, ColoredConfig> for Colorization {
    fn change(self, records: &mut VecRecords<CellInfo<String>>, cfg: &mut ColoredConfig, entity: Entity) {
        let (rows, columns) = (records.count_rows(), records.count_columns());
        for (row, col) in entity.iter(rows, columns) {
            let status = records[row][col].as_ref();
            let color = vm_status_color(status);
            cfg.set_color(Entity::Cell(row, col), color.into());
        }
    }
}

fn vm_status_color(status: &str) -> Color {
    match status {
        "VM deallocated" => Color::BG_RED | Color::FG_BLACK,
        "VM deallocating" | "VM starting" => Color::BG_YELLOW | Color::FG_BLACK,
        "VM running" => Color::BG_GREEN | Color::FG_BLACK,
        _ => Color::default(),
    }
}
