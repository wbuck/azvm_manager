use tabled::{Table, Tabled, grid::records::into_records::truncate_records::ExactValue};
use azure_mgmt_resources::models::{ResourceGroup, ResourceGroupProperties};

use std::borrow::Cow;
use std::iter;

use crate::{Output, get_style};


pub fn display_rg(out: Output<ResourceGroup>) {
    let mut table = match out {
        Output::Single(group) => Table::new(iter::once(Row(group))),
        Output::Multiple(groups) => Table::new(groups.iter().map(|group| Row(group)))
    };

    table.with(get_style());
    println!("{table}");
}

struct Row<'a>(&'a ResourceGroup);

impl<'a> Tabled for Row<'a> {
    const LENGTH: usize = 3;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        vec![ 
            Cow::from(self.0.name.as_deref().unwrap_or("")), 
            Cow::from(&self.0.location),
            Cow::from(Self::get_state(&self.0.properties))   
        ]
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![
            Cow::from("Name"), 
            Cow::from("Location"),
            Cow::from("Provisioning State")
        ]
    }
}

impl<'a> Row<'a> {
    fn get_state(state: &Option<ResourceGroupProperties>) -> &str {
        match state {
            Some(properties) => properties.provisioning_state.as_deref().unwrap_or(""),
            None => ""
        }
    }
}
