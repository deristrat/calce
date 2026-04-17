from __future__ import annotations

import json
import logging
import os
from datetime import date
from pathlib import Path

import anthropic
import calce
from dotenv import load_dotenv
from fastapi import Depends, FastAPI, Request
from fastapi.responses import HTMLResponse, JSONResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel
from sse_starlette.sse import EventSourceResponse

from .auth import get_current_user
from .tools import TOOL_DEFINITIONS, UserContext, execute_tool

MODEL = "claude-sonnet-4-20250514"

logging.basicConfig(level=logging.INFO, format="%(levelname)s:  %(name)s - %(message)s", force=True)
log = logging.getLogger("calce-ai")

# Load .env from project root when running from a source checkout. In container
# deployments the file does not exist (env vars are injected by the runtime).
_parents = Path(__file__).resolve().parents
if len(_parents) > 3:
    load_dotenv(_parents[3] / ".env")

app = FastAPI(title="Calce AI")


@app.exception_handler(calce.InvalidCredentialsError)
def handle_invalid_credentials(request: Request, exc: calce.InvalidCredentialsError):
    return JSONResponse(status_code=401, content={"detail": str(exc)})


@app.exception_handler(calce.AccountLockedError)
def handle_account_locked(request: Request, exc: calce.AccountLockedError):
    return JSONResponse(status_code=423, content={"detail": str(exc)})


@app.exception_handler(calce.InvalidTokenError)
def handle_invalid_token(request: Request, exc: calce.InvalidTokenError):
    return JSONResponse(status_code=401, content={"detail": str(exc)})


# ── Startup ─────────────────────────────────────────────────────────────

database_url = os.environ.get("DATABASE_URL", "")


@app.on_event("startup")
def startup():
    log.info("Connecting to database...")
    app.state.auth_service = calce.AuthService(database_url)
    log.info("Auth service ready")

    log.info("Loading market data and user data from database...")
    app.state.data_service = calce.DataService(database_url)
    stats = app.state.data_service.data_stats()
    log.info(
        "Data loaded: %d users, %d instruments, %d trades, %d prices, %d FX rates",
        stats.user_count,
        stats.instrument_count,
        stats.trade_count,
        stats.price_count,
        stats.fx_rate_count,
    )

    app.state.anthropic = anthropic.Anthropic()
    log.info("Startup complete — server ready")


# ── Auth endpoints ──────────────────────────────────────────────────────


class LoginRequest(BaseModel):
    email: str
    password: str


class RefreshRequest(BaseModel):
    refresh_token: str


class LogoutRequest(BaseModel):
    refresh_token: str


@app.post("/auth/login")
def login(body: LoginRequest, request: Request):
    auth: calce.AuthService = request.app.state.auth_service
    tokens = auth.login(body.email, body.password)
    return {
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "token_type": "Bearer",
        "expires_in": tokens.expires_in,
    }


@app.post("/auth/refresh")
def refresh(body: RefreshRequest, request: Request):
    auth: calce.AuthService = request.app.state.auth_service
    tokens = auth.refresh(body.refresh_token)
    return {
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "token_type": "Bearer",
        "expires_in": tokens.expires_in,
    }


@app.post("/auth/logout")
def logout(body: LogoutRequest, request: Request):
    auth: calce.AuthService = request.app.state.auth_service
    auth.logout(body.refresh_token)
    return {"logged_out": True}


# ── Chat endpoint (SSE streaming) ──────────────────────────────────────


class ChatRequest(BaseModel):
    message: str
    history: list[dict] = []
    base_currency: str = "SEK"


@app.post("/chat")
async def chat(
    body: ChatRequest,
    request: Request,
    user: calce.SecurityContext = Depends(get_current_user),
):
    ds: calce.DataService = request.app.state.data_service
    client: anthropic.Anthropic = request.app.state.anthropic

    ctx = UserContext(
        user_id=user.user_id,
        role=user.role,
        base_currency=body.base_currency,
        as_of_date=date.today(),
    )

    access_note = (
        "You have admin access and can view any user."
        if ctx.role == "admin"
        else "You can only view this user's own portfolio."
    )
    system_prompt = (
        f"You are a financial portfolio analyst assistant for the Calce calculation engine. "
        f'You are currently helping user "{ctx.user_id}" (role: {ctx.role}). '
        f"Base currency: {ctx.base_currency}. Date: {ctx.as_of_date}. "
        f"{access_note} "
        f"Use your tools to answer questions about portfolios and market data. "
        f"Format numbers clearly and provide analytical insights."
    )

    messages = list(body.history)
    messages.append({"role": "user", "content": body.message})

    async def event_stream():
        nonlocal messages
        try:
            while True:
                collected_text = ""
                tool_calls = []

                with client.messages.stream(
                    model=MODEL,
                    max_tokens=4096,
                    system=system_prompt,
                    tools=TOOL_DEFINITIONS,
                    messages=messages,
                ) as stream:
                    for event in stream:
                        if event.type == "content_block_start":
                            if event.content_block.type == "tool_use":
                                tool_calls.append(
                                    {
                                        "id": event.content_block.id,
                                        "name": event.content_block.name,
                                        "input_json": "",
                                    }
                                )
                        elif event.type == "content_block_delta":
                            if event.delta.type == "text_delta":
                                collected_text += event.delta.text
                                yield {
                                    "event": "text",
                                    "data": json.dumps({"content": event.delta.text}),
                                }
                            elif event.delta.type == "input_json_delta":
                                if tool_calls:
                                    tool_calls[-1]["input_json"] += event.delta.partial_json

                    response = stream.get_final_message()

                if response.stop_reason != "tool_use":
                    break

                # Execute tool calls
                messages.append({"role": "assistant", "content": response.content})
                tool_results = []
                for tc in tool_calls:
                    tool_input = json.loads(tc["input_json"]) if tc["input_json"] else {}
                    yield {
                        "event": "tool_call",
                        "data": json.dumps({"name": tc["name"]}),
                    }
                    result = execute_tool(ds, ctx, tc["name"], tool_input)
                    tool_results.append(
                        {
                            "type": "tool_result",
                            "tool_use_id": tc["id"],
                            "content": result,
                        }
                    )
                messages.append({"role": "user", "content": tool_results})

            yield {"event": "done", "data": "{}"}
        except Exception as e:
            yield {"event": "error", "data": json.dumps({"message": str(e)})}

    return EventSourceResponse(event_stream())


# ── Static files & frontend ────────────────────────────────────────────

STATIC_DIR = Path(__file__).parent / "static"


@app.get("/", response_class=HTMLResponse)
def index():
    return (STATIC_DIR / "index.html").read_text()


app.mount("/static", StaticFiles(directory=str(STATIC_DIR)), name="static")
