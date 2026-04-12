//! Access control checks.
//!
//! Centralizes all "can user X do Y" decisions in one reviewable module.

use crate::auth::{Role, SecurityContext};
use calce_core::domain::user::UserId;

/// Can the authenticated user access the target user's data?
///
/// Rules:
/// - Unrestricted admin (human, no `org_id`) can access any user's data
/// - Org-scoped admin (API key) is denied here — route handlers must
///   verify org membership with a DB lookup before granting access
/// - A regular user can only access their own data
#[must_use]
pub(crate) fn can_access_user_data(ctx: &SecurityContext, target: &UserId) -> bool {
    if ctx.role == Role::Admin && ctx.org_id.is_none() {
        return true;
    }
    ctx.user_id == *target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_can_access_own_data() {
        let alice = UserId::new("alice");
        let ctx = SecurityContext::new(alice.clone(), Role::User);
        assert!(can_access_user_data(&ctx, &alice));
    }

    #[test]
    fn user_cannot_access_other_data() {
        let alice = UserId::new("alice");
        let bob = UserId::new("bob");
        let ctx = SecurityContext::new(bob, Role::User);
        assert!(!can_access_user_data(&ctx, &alice));
    }

    #[test]
    fn admin_can_access_any_data() {
        let alice = UserId::new("alice");
        let ctx = SecurityContext::system();
        assert!(can_access_user_data(&ctx, &alice));
    }

    #[test]
    fn org_scoped_admin_cannot_access_arbitrary_user_data() {
        let alice = UserId::new("alice");
        let ctx =
            SecurityContext::new(UserId::new("org1"), Role::Admin).with_org("org1".to_owned());
        // Org-scoped admins are denied by default — route handlers must
        // verify org membership via DB before granting access.
        assert!(!can_access_user_data(&ctx, &alice));
    }
}
