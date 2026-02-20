use crate::domain::user::UserId;

/// Access level for a security context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// Full access to all users' data.
    Admin,
    /// Access limited to own data.
    User,
}

/// Security context passed to all data access operations.
#[derive(Clone, Debug)]
pub struct SecurityContext {
    /// The authenticated user.
    pub user_id: UserId,
    /// The user's role.
    pub role: Role,
}

impl SecurityContext {
    /// Create a new security context.
    #[must_use]
    pub fn new(user_id: UserId, role: Role) -> Self {
        SecurityContext { user_id, role }
    }

    /// System-level context with admin privileges.
    #[must_use]
    pub fn system() -> Self {
        SecurityContext {
            user_id: UserId::new("system"),
            role: Role::Admin,
        }
    }

    /// Check if this context can access the given user's data.
    #[must_use]
    pub fn can_access(&self, target: &UserId) -> bool {
        self.role == Role::Admin || self.user_id == *target
    }
}
