"""Shared dev admin user creation for seed and import scripts."""

import argon2

ADMIN_EMAIL = "admin@njorda.se"
ADMIN_PASSWORD = "protectme"
ADMIN_EXTERNAL_ID = "admin"


def ensure_admin_user(cur, conn):
    """Insert the local dev admin user and credentials. Idempotent."""
    ph = argon2.PasswordHasher(time_cost=2, memory_cost=19456, parallelism=1, type=argon2.Type.ID)

    cur.execute(
        "INSERT INTO users (external_id, email, name, role) VALUES (%s, %s, %s, 'admin') "
        "ON CONFLICT (external_id) DO UPDATE SET email = EXCLUDED.email, role = 'admin' "
        "RETURNING id",
        (ADMIN_EXTERNAL_ID, ADMIN_EMAIL, "Admin"),
    )
    admin_id = cur.fetchone()[0]

    admin_hash = ph.hash(ADMIN_PASSWORD)
    cur.execute(
        "INSERT INTO user_credentials (user_id, password_hash) VALUES (%s, %s) "
        "ON CONFLICT (user_id) DO UPDATE SET password_hash = EXCLUDED.password_hash",
        (admin_id, admin_hash),
    )
    conn.commit()
    return admin_id, ph
