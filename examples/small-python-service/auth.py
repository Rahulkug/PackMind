"""Session authentication for the service API."""

import hashlib
import time

SESSION_TTL_SECONDS = 3600
_sessions: dict[str, float] = {}


def create_session(user_id: str) -> str:
    """Create a session token for a user."""
    token = hashlib.sha256(f"{user_id}:{time.time()}".encode()).hexdigest()
    _sessions[token] = time.time()
    return token


def validate_session(token: str) -> bool:
    """Check whether a session token is current."""
    started = _sessions.get(token)
    if started is None:
        return False
    return time.time() - started < SESSION_TTL_SECONDS


def require_auth(token: str) -> None:
    """Raise PermissionError unless the token is a valid session."""
    if not validate_session(token):
        raise PermissionError("authentication required")
