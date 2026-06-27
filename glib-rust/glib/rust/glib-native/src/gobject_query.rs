//! `gobject-query.c` compatibility helpers.

use crate::gtype::{type_children, type_depth, type_from_name, type_name, GType};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectQuery {
    pub type_id: GType,
    pub name: String,
    pub depth: u32,
    pub children: Vec<GType>,
}

#[must_use]
pub fn query_type(type_id: GType) -> Option<ObjectQuery> {
    let name = type_name(type_id)?;
    Some(ObjectQuery {
        type_id,
        name,
        depth: type_depth(type_id),
        children: type_children(type_id),
    })
}

#[must_use]
pub fn query_type_by_name(name: &str) -> Option<ObjectQuery> {
    let type_id = type_from_name(name);
    if type_id == 0 {
        None
    } else {
        query_type(type_id)
    }
}
