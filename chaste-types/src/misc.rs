// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

macro_rules! partial_eq_field {
    ($own:ty, $field:ident, $other:ty) => {
        impl PartialEq<$other> for $own {
            fn eq(&self, other: &$other) -> bool {
                self.$field.eq(other)
            }
        }
        impl PartialEq<&$other> for $own {
            fn eq(&self, other: &&$other) -> bool {
                self.$field.eq(*other)
            }
        }
        impl PartialEq<$other> for &$own {
            fn eq(&self, other: &$other) -> bool {
                self.$field.eq(other)
            }
        }
        impl PartialEq<$own> for $other {
            fn eq(&self, own: &$own) -> bool {
                self.eq(&own.$field)
            }
        }
        impl PartialEq<&$own> for $other {
            fn eq(&self, own: &&$own) -> bool {
                self.eq(&own.$field)
            }
        }
        impl PartialEq<Option<$other>> for $own {
            fn eq(&self, other: &Option<$other>) -> bool {
                other.as_ref().is_some_and(|o| self.$field.eq(o))
            }
        }
        impl PartialEq<Option<&$other>> for $own {
            fn eq(&self, other: &Option<&$other>) -> bool {
                other.is_some_and(|o| self.$field.eq(o))
            }
        }
    };
}

pub(crate) use partial_eq_field;
