from __future__ import annotations

from dataclasses import dataclass
from datetime import date, timedelta

import calce

from . import formatting

TOOL_DEFINITIONS = [
    {
        "name": "get_portfolio_report",
        "description": (
            "Get a comprehensive portfolio report including market value, "
            "value changes (daily/weekly/yearly/YTD), asset type allocation, "
            "and sector allocation."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "user_id": {
                    "type": "string",
                    "description": "User ID to get report for. Defaults to current user.",
                },
            },
        },
    },
    {
        "name": "get_market_value",
        "description": "Get the current market value of a user's portfolio with per-position breakdown.",
        "input_schema": {
            "type": "object",
            "properties": {
                "user_id": {
                    "type": "string",
                    "description": "User ID. Defaults to current user.",
                },
            },
        },
    },
    {
        "name": "calculate_volatility",
        "description": (
            "Calculate historical realized volatility (annualized and daily) "
            "for a specific instrument."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "instrument_id": {
                    "type": "string",
                    "description": "Instrument ticker (e.g. 'AAPL')",
                },
                "lookback_days": {
                    "type": "integer",
                    "description": "Calendar days of history. Default 1095 (3 years).",
                },
            },
            "required": ["instrument_id"],
        },
    },
    {
        "name": "search_instruments",
        "description": (
            "Search for instruments by ticker, name, or type. "
            "Returns matching instruments with their type and currency."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search term (matches ticker, name, or type; case-insensitive)",
                },
            },
            "required": ["query"],
        },
    },
    {
        "name": "get_price_history",
        "description": "Get historical price data for an instrument over a specified period.",
        "input_schema": {
            "type": "object",
            "properties": {
                "instrument_id": {
                    "type": "string",
                    "description": "Instrument ticker",
                },
                "days": {
                    "type": "integer",
                    "description": "Number of days of history. Default 30.",
                },
            },
            "required": ["instrument_id"],
        },
    },
    {
        "name": "get_data_overview",
        "description": (
            "Get an overview of available data: counts of users, instruments, "
            "prices, FX rates, and trades."
        ),
        "input_schema": {
            "type": "object",
            "properties": {},
        },
    },
]


@dataclass
class UserContext:
    user_id: str
    role: str  # "user" or "admin"
    base_currency: str
    as_of_date: date


def _check_user_access(ctx: UserContext, target_user_id: str) -> str | None:
    """Return an error message if access is denied, else None."""
    if ctx.role != "admin" and target_user_id != ctx.user_id:
        return f"Access denied: you can only view your own data (you are {ctx.user_id})."
    return None


def execute_tool(
    ds: calce.DataService,
    ctx: UserContext,
    tool_name: str,
    tool_input: dict,
) -> str:
    """Execute a tool call and return the result as text."""
    try:
        return _dispatch(ds, ctx, tool_name, tool_input)
    except calce.NoTradesFoundError:
        return "No trades found for this user."
    except calce.CalceError as e:
        return f"Calculation error: {e}"
    except Exception as e:
        return f"Error: {e}"


def _dispatch(
    ds: calce.DataService,
    ctx: UserContext,
    tool_name: str,
    tool_input: dict,
) -> str:
    if tool_name == "get_portfolio_report":
        uid = tool_input.get("user_id", ctx.user_id)
        if err := _check_user_access(ctx, uid):
            return err
        engine = ds.engine(uid, ctx.base_currency, ctx.as_of_date)
        report = engine.portfolio_report()
        return formatting.format_portfolio_report(report)

    if tool_name == "get_market_value":
        uid = tool_input.get("user_id", ctx.user_id)
        if err := _check_user_access(ctx, uid):
            return err
        engine = ds.engine(uid, ctx.base_currency, ctx.as_of_date)
        result = engine.market_value()
        return formatting.format_market_value(result)

    if tool_name == "calculate_volatility":
        instrument_id = tool_input["instrument_id"]
        lookback = tool_input.get("lookback_days", 1095)
        engine = ds.engine(ctx.user_id, ctx.base_currency, ctx.as_of_date)
        result = engine.volatility(instrument_id, lookback)
        return formatting.format_volatility(result, instrument_id)

    if tool_name == "search_instruments":
        query = tool_input["query"]
        instruments = ds.search_instruments(query)
        return formatting.format_instruments(instruments)

    if tool_name == "get_price_history":
        instrument_id = tool_input["instrument_id"]
        days = tool_input.get("days", 30)
        from_date = ctx.as_of_date - timedelta(days=days)
        points = ds.get_price_history(instrument_id, from_date, ctx.as_of_date)
        return formatting.format_price_history(points, instrument_id)

    if tool_name == "get_data_overview":
        stats = ds.data_stats()
        return formatting.format_data_stats(stats, ctx)

    return f"Unknown tool: {tool_name}"
