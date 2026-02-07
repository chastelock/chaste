// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use std::collections::{btree_map, BTreeMap};

pub struct Candidates<'a, T> {
    first_value: &'a str,
    range: btree_map::Range<'a, (&'a str, &'a str), T>,
}

impl<'a, T> Candidates<'a, T> {
    pub fn new(first_value: &'a str, btree: &'a BTreeMap<(&'a str, &'a str), T>) -> Self {
        Candidates {
            first_value,
            range: btree.range((first_value, "")..),
        }
    }
}

impl<'a, T> Iterator for Candidates<'a, T> {
    type Item = (&'a (&'a str, &'a str), &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let Some(item) = self.range.next() else {
            return None;
        };
        if item.0 .0 != self.first_value {
            return None;
        }
        Some(item)
    }
}
