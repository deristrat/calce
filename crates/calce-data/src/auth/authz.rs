//! Access control — "can user X do Y" decisions and `require_*` guards.
//!
//! All authorization rules live here. Route handlers and stores call into
//! this module; no authz logic belongs anywhere else.

use super::{Role, SecurityContext};
use crate::error::{DataError, DataResult};
use calce_core::domain::user::UserId;

/// Can the authenticated user access the target user's data?
///
/// Rules:
/// - Unrestricted admin (human, no `org_id`) can access any user's data
/// - Org-scoped admin (API key) is denied here — route handlers must
///   verify org membership with a DB lookup before granting access
/// - A regular user can only access their own data
#[must_use]
pub fn can_access_user_data(ctx: &SecurityContext, target: &UserId) -> bool {
    if ctx.role == Role::Admin && ctx.org_id.is_none() {
        return true;
    }
    ctx.user_id == *target
}

/// Require unrestricted admin (human user, not org-scoped API key).
///
/// # Errors
///
/// Returns `Unauthorized` if the caller is not an unrestricted admin.
pub fn require_admin(ctx: &SecurityContext) -> DataResult<()> {
    if ctx.is_unrestricted_admin() {
        Ok(())
    } else {
        Err(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new("*"),
        })
    }
}

/// Require admin with access to a specific organization.
///
/// Human admins pass unconditionally; org-scoped admins (API keys) must
/// belong to the requested org.
///
/// # Errors
///
/// Returns `Unauthorized` if the caller is not an admin or is scoped to
/// a different organization.
pub fn require_org_admin(ctx: &SecurityContext, target_org: &str) -> DataResult<()> {
    if !ctx.is_admin() {
        return Err(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new(target_org),
        });
    }
    if let Some(ref org_id) = ctx.org_id
        && org_id != target_org
    {
        return Err(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: UserId::new(target_org),
        });
    }
    Ok(())
}

/// Require the caller to have access to `target`'s user data.
///
/// # Errors
///
/// Returns `Unauthorized` if the caller cannot access the target.
pub fn require_access(ctx: &SecurityContext, target: &str) -> DataResult<()> {
    let target_id = UserId::new(target);
    if can_access_user_data(ctx, &target_id) {
        Ok(())
    } else {
        Err(DataError::Unauthorized {
            requester: ctx.user_id.clone(),
            target: target_id,
        })
    }
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
        assert!(!can_access_user_data(&ctx, &alice));
    }

    #[test]
    fn require_admin_allows_unrestricted_admin() {
        let ctx = SecurityContext::system();
        assert!(require_admin(&ctx).is_ok());
    }

    #[test]
    fn require_admin_rejects_regular_user() {
        let ctx = SecurityContext::new(UserId::new("alice"), Role::User);
        assert!(matches!(
            require_admin(&ctx),
            Err(DataError::Unauthorized { .. })
        ));
    }

    #[test]
    fn require_admin_rejects_org_scoped_admin() {
        let ctx = SecurityContext::new(UserId::new("svc"), Role::Admin).with_org("org1".to_owned());
        assert!(matches!(
            require_admin(&ctx),
            Err(DataError::Unauthorized { .. })
        ));
    }

    #[test]
    fn require_org_admin_allows_matching_org() {
        let ctx = SecurityContext::new(UserId::new("svc"), Role::Admin).with_org("org1".to_owned());
        assert!(require_org_admin(&ctx, "org1").is_ok());
    }

    #[test]
    fn require_org_admin_rejects_other_org() {
        let ctx = SecurityContext::new(UserId::new("svc"), Role::Admin).with_org("org1".to_owned());
        assert!(matches!(
            require_org_admin(&ctx, "org2"),
            Err(DataError::Unauthorized { .. })
        ));
    }

    #[test]
    fn require_org_admin_allows_unrestricted_admin() {
        let ctx = SecurityContext::system();
        assert!(require_org_admin(&ctx, "org1").is_ok());
    }

    #[test]
    fn require_org_admin_rejects_regular_user() {
        let ctx = SecurityContext::new(UserId::new("alice"), Role::User);
        assert!(matches!(
            require_org_admin(&ctx, "org1"),
            Err(DataError::Unauthorized { .. })
        ));
    }

    #[test]
    fn require_access_allows_self() {
        let ctx = SecurityContext::new(UserId::new("alice"), Role::User);
        assert!(require_access(&ctx, "alice").is_ok());
    }

    #[test]
    fn require_access_rejects_other_user() {
        let ctx = SecurityContext::new(UserId::new("alice"), Role::User);
        assert!(matches!(
            require_access(&ctx, "bob"),
            Err(DataError::Unauthorized { .. })
        ));
    }
}
