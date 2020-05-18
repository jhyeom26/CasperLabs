mod claim_request_list;
mod request_queue;
mod requests;

use contract::contract_api::storage;

use crate::constants::local_keys;

use claim_request_list::ClaimRequestList;
use request_queue::{RequestKey, RequestQueue};
pub use requests::{ClaimRequest, RedelegateRequestKey, UndelegateRequestKey};

pub struct ContractQueue;
pub enum DelegationKind {
    Undelegate,
    Redelegate,
}

impl ContractQueue {
    pub fn read_delegation_requests<T: RequestKey + Default>(
        kind: DelegationKind,
    ) -> RequestQueue<T> {
        let key = match kind {
            DelegationKind::Undelegate => local_keys::UNDELEGATE_REQUEST_QUEUE,
            DelegationKind::Redelegate => local_keys::REDELEGATE_REQUEST_QUEUE,
        };
        storage::read_local(&key)
            .unwrap_or_default()
            .unwrap_or_default()
    }
    pub fn write_delegation_requests<T: RequestKey + Default>(
        kind: DelegationKind,
        queue: RequestQueue<T>,
    ) {
        let key = match kind {
            DelegationKind::Undelegate => local_keys::UNDELEGATE_REQUEST_QUEUE,
            DelegationKind::Redelegate => local_keys::REDELEGATE_REQUEST_QUEUE,
        };
        storage::write_local(key, queue);
    }

    pub fn read_claim_requests() -> ClaimRequestList {
        storage::read_local(&local_keys::CLAIM_REQUESTS)
            .unwrap_or_default()
            .unwrap_or_default()
    }
    pub fn write_claim_requests(list: ClaimRequestList) {
        storage::write_local(local_keys::CLAIM_REQUESTS, list);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use types::{account::PublicKey, system_contract_errors::pos::Error, BlockTime, U512};

    use super::{RequestQueue, UndelegateRequestKey};
    use crate::pop_impl::request_pool::request_queue::RequestQueueEntry;

    const KEY1: [u8; 32] = [1; 32];
    const KEY2: [u8; 32] = [2; 32];
    const KEY3: [u8; 32] = [3; 32];
    const KEY4: [u8; 32] = [4; 32];

    #[test]
    fn test_request_queue_push() {
        let delegator = PublicKey::ed25519_from(KEY1);
        let validator_1 = PublicKey::ed25519_from(KEY2);
        let validator_2 = PublicKey::ed25519_from(KEY3);
        let validator_3 = PublicKey::ed25519_from(KEY4);

        let mut queue: RequestQueue<UndelegateRequestKey> = Default::default();
        assert_eq!(
            Ok(()),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_1),
                U512::from(5),
                BlockTime::new(100)
            )
        );
        assert_eq!(
            Ok(()),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_2),
                U512::from(5),
                BlockTime::new(101)
            )
        );
        assert_eq!(
            Err(Error::MultipleRequests),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_1),
                U512::from(6),
                BlockTime::new(102)
            )
        );
        assert_eq!(
            Err(Error::TimeWentBackwards),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_3),
                U512::from(5),
                BlockTime::new(100)
            )
        );
    }

    #[test]
    fn test_request_queue_pop_due() {
        let delegator = PublicKey::ed25519_from(KEY1);
        let validator_1 = PublicKey::ed25519_from(KEY2);
        let validator_2 = PublicKey::ed25519_from(KEY3);
        let validator_3 = PublicKey::ed25519_from(KEY4);

        let mut queue: RequestQueue<UndelegateRequestKey> = Default::default();
        assert_eq!(
            Ok(()),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_1),
                U512::from(5),
                BlockTime::new(100)
            )
        );
        assert_eq!(
            Ok(()),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_2),
                U512::from(5),
                BlockTime::new(101)
            )
        );
        assert_eq!(
            Ok(()),
            queue.push(
                UndelegateRequestKey::new(delegator, validator_3),
                U512::from(5),
                BlockTime::new(102)
            )
        );
        assert_eq!(
            vec![
                RequestQueueEntry {
                    request_key: UndelegateRequestKey::new(delegator, validator_1),
                    amount: U512::from(5),
                    timestamp: BlockTime::new(100)
                },
                RequestQueueEntry {
                    request_key: UndelegateRequestKey::new(delegator, validator_2),
                    amount: U512::from(5),
                    timestamp: BlockTime::new(101)
                },
            ],
            queue.pop_due(BlockTime::new(101))
        );
        assert_eq!(
            vec![RequestQueueEntry {
                request_key: UndelegateRequestKey::new(delegator, validator_3),
                amount: U512::from(5),
                timestamp: BlockTime::new(102)
            },],
            queue.pop_due(BlockTime::new(105))
        );
    }
}