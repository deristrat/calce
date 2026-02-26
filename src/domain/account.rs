use super::currency::Currency;
use super::user::UserId;

string_id!(AccountId);

#[derive(Clone, Debug)]
pub struct Account {
    pub id: AccountId,
    pub owner: UserId,
    pub currency: Currency,
    pub label: String,
}
