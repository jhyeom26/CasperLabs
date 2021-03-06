use alloc::{
    collections::{btree_map::Iter, BTreeMap},
    vec::Vec,
};

use types::{
    account::PublicKey,
    system_contract_errors::pos::{Error, Result},
    U512,
};

use crate::{constants::sys_params, store};

pub struct Delegations {
    table: BTreeMap<DelegationKey, U512>,
    total_amount: U512,
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Copy)]
pub struct DelegationKey {
    pub delegator: PublicKey,
    pub validator: PublicKey,
}
/*
Retriving the validator list, delegating_amount of a delegator and
delegated_amount of a validator impelementation is currently achieved by
iterating a whole delegation map.
They are fairly expensive operations so use them carefully.
*/
impl Delegations {
    pub fn new(table: BTreeMap<DelegationKey, U512>) -> Self {
        let total_amount = table.values().fold(U512::zero(), |acc, x| acc + x);
        Self {
            table,
            total_amount,
        }
    }

    pub fn iter(&self) -> Iter<DelegationKey, U512> {
        self.table.iter()
    }

    pub fn validators(&self) -> Vec<(PublicKey, U512)> {
        let mut validators = BTreeMap::default();
        for (
            DelegationKey {
                delegator: _,
                validator,
            },
            amount,
        ) in self.table.iter()
        {
            validators
                .entry(*validator)
                .and_modify(|x| *x += *amount)
                .or_insert(*amount);
        }

        let mut validators = validators.into_iter().collect::<Vec<_>>();

        // sort by descending order and truncate
        validators.sort_by(|a, b| b.1.cmp(&a.1));
        validators.truncate(sys_params::MAX_VALIDATORS);

        validators
    }

    pub fn total_amount(&self) -> U512 {
        self.total_amount
    }

    pub fn delegation(&self, delegator: &PublicKey, validator: &PublicKey) -> Result<U512> {
        self.table
            .get(&DelegationKey {
                delegator: *delegator,
                validator: *validator,
            })
            .cloned()
            .ok_or(Error::DelegationsNotFound)
    }

    pub fn delegating_amount(&self, delegator: &PublicKey) -> U512 {
        self.table
            .iter()
            .map(|x| {
                if x.0.delegator == *delegator {
                    *x.1
                } else {
                    U512::zero()
                }
            })
            .fold(U512::zero(), |acc, x| acc + x)
    }

    pub fn delegated_amount(&self, validator: &PublicKey) -> U512 {
        self.table
            .iter()
            .filter(|x| x.0.validator == *validator)
            .map(|x| x.1)
            .fold(U512::zero(), |acc, x| acc + *x)
    }

    pub fn delegate(
        &mut self,
        delegator: &PublicKey,
        validator: &PublicKey,
        amount: U512,
    ) -> Result<()> {
        let key = DelegationKey {
            delegator: *delegator,
            validator: *validator,
        };
        // if request is not self-delegation and validator is not self-delegated, return error
        if *delegator != *validator && self.delegation(validator, validator).is_err() {
            return Err(Error::NotSelfDelegated);
        }

        // validate amount
        {
            let bonding_amount = store::read_bonding_amount(delegator);
            let delegating_amount = self.delegating_amount(delegator);
            if amount > bonding_amount.saturating_sub(delegating_amount) {
                return Err(Error::DelegateTooLarge);
            }
        }

        // update table
        self.table
            .entry(key)
            .and_modify(|x| *x += amount)
            .or_insert(amount);

        // update total amount
        self.total_amount += amount;

        Ok(())
    }

    pub fn undelegate(
        &mut self,
        delegator: &PublicKey,
        validator: &PublicKey,
        maybe_amount: Option<U512>,
    ) -> Result<U512> {
        let key = DelegationKey {
            delegator: *delegator,
            validator: *validator,
        };

        // update table
        let undelegate_amount = match maybe_amount {
            // undelegate all
            None => self.table.remove(&key).ok_or(Error::DelegationsNotFound)?,
            Some(amount) => {
                let delegation = self.table.get_mut(&key);
                match delegation {
                    Some(delegation) if *delegation > amount => {
                        *delegation -= amount;
                        amount
                    }
                    Some(delegation) if *delegation == amount => {
                        self.table.remove(&key).ok_or(Error::DelegationsNotFound)?
                    }
                    Some(_) => return Err(Error::UndelegateTooLarge),
                    None => return Err(Error::DelegationsNotFound),
                }
            }
        };

        // update total amount
        self.total_amount = self.total_amount.saturating_sub(undelegate_amount);

        Ok(undelegate_amount)
    }

    pub fn redelegate(
        &mut self,
        delegator: &PublicKey,
        src_validator: &PublicKey,
        dest_validator: &PublicKey,
        maybe_amount: Option<U512>,
    ) -> Result<()> {
        // update table
        {
            // undelegate
            let key = DelegationKey {
                delegator: *delegator,
                validator: *src_validator,
            };
            let undelegate_amount = match maybe_amount {
                // undelegate all
                None => self.table.remove(&key).ok_or(Error::DelegationsNotFound)?,
                Some(amount) => {
                    let delegation = self.table.get_mut(&key);
                    match delegation {
                        Some(delegation) if *delegation > amount => {
                            *delegation -= amount;
                            amount
                        }
                        Some(delegation) if *delegation == amount => {
                            self.table.remove(&key).ok_or(Error::DelegationsNotFound)?
                        }
                        Some(_) => return Err(Error::UndelegateTooLarge),
                        None => return Err(Error::DelegationsNotFound),
                    }
                }
            };

            // delegate
            let key = DelegationKey {
                delegator: *delegator,
                validator: *dest_validator,
            };
            self.table
                .entry(key)
                .and_modify(|x| *x += undelegate_amount)
                .or_insert(undelegate_amount);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;

    use types::{account::PublicKey, U512};

    use super::{DelegationKey, Delegations};
    use crate::constants::sys_params::MAX_VALIDATORS;

    #[test]
    fn test_validators() {
        let mut table = BTreeMap::default();
        for i in 1..=(MAX_VALIDATORS + 1) {
            table.insert(
                DelegationKey {
                    delegator: PublicKey::ed25519_from([i as u8; 32]),
                    validator: PublicKey::ed25519_from([i as u8; 32]),
                },
                U512::from(i),
            );
        }
        let delegations = Delegations::new(table);
        let validators = delegations.validators();

        assert_eq!(validators.len(), MAX_VALIDATORS);
        // the least element([1u8;32]) is truncated.
        assert_eq!(
            validators
                .last()
                .cloned()
                .expect("validators shouldn't be empty"),
            (PublicKey::ed25519_from([2u8; 32]), U512::from(2))
        );
    }
}
