use crate::{DeviceCell, domain::parse_pin_map_indices, domain::pin_map_property_name};
use smallvec::SmallVec;

use super::super::types::WireId;

pub(crate) type WireSet = SmallVec<[WireId; 1]>;

pub(super) fn bel_slot(bel: &str) -> Option<usize> {
    bel.chars()
        .rev()
        .find(|ch| ch.is_ascii_digit())
        .and_then(|ch| ch.to_digit(10))
        .map(|digit| digit as usize)
}

pub(super) fn pin_map_indices(cell: &DeviceCell, logical_index: usize) -> Vec<usize> {
    let key = pin_map_property_name(logical_index);
    let value = cell
        .properties
        .iter()
        .find(|property| property.key.eq_ignore_ascii_case(&key))
        .map(|property| property.value.as_str());
    parse_pin_map_indices(value, logical_index)
}
