use tabled::{Table, Tabled, grid::records::into_records::truncate_records::ExactValue};
use azure_mgmt_subscription::{
    models::subscription::State, 
    models::{Subscription, SubscriptionPolicies}
};
use std::borrow::Cow;
use std::iter;

use crate::{Output, get_style};


pub fn display_sub(out: Output<Subscription>) {
    let mut table = match out {
        Output::Single(sub) => Table::new(iter::once(Row(sub))),
        Output::Multiple(subs) => Table::new(subs.iter().map(|sub| Row(sub)))
    };

    table.with(get_style());
    println!("{table}");
}

struct Row<'a>(&'a Subscription);

impl<'a> Tabled for Row<'a> {
    const LENGTH: usize = 3;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        vec![ 
            Cow::from(self.0.subscription_id.as_deref().unwrap_or("")), 
            Cow::from(self.0.display_name.as_deref().unwrap_or("")),
            Cow::from(Self::get_state(&self.0.state))   
        ]
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![
            Cow::from("Subscription ID"), 
            Cow::from("Name"),
            Cow::from("State")
        ]
    }
}

impl<'a> Row<'a> {
    fn get_state(state: &Option<State>) -> &'static str {
        match state {
            Some(State::Enabled) => "Enabled",
            Some(State::Warned) => "Warned",
            Some(State::PastDue) => "Past Due",
            Some(State::Disabled) => "Disabled",
            Some(State::Deleted) => "Deleted",
            None => "",
        }
    }
}
