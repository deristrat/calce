from __future__ import annotations

import os
import sys
from datetime import date
from pathlib import Path

import anthropic
import calce
from dotenv import load_dotenv

from .tools import TOOL_DEFINITIONS, UserContext, execute_tool

MODEL = "claude-sonnet-4-20250514"


def main():
    # Load .env from project root
    load_dotenv(Path(__file__).resolve().parents[3] / ".env")

    print("Calce AI - Portfolio Analysis Assistant")
    print("=" * 40)
    print()

    # Check API key early
    if not os.environ.get("ANTHROPIC_API_KEY"):
        print("Error: ANTHROPIC_API_KEY environment variable is required.")
        sys.exit(1)

    # Connect and load data
    database_url = os.environ.get("DATABASE_URL")
    if not database_url:
        print("Error: DATABASE_URL environment variable is required.")
        sys.exit(1)

    print("Loading data from database...")
    try:
        ds = calce.DataService(database_url)
    except calce.DataLoadError as e:
        print(f"Error: {e}")
        sys.exit(1)

    stats = ds.data_stats()
    print(
        f"  {stats.user_count} users, {stats.instrument_count} instruments, "
        f"{stats.trade_count:,} trades, {stats.price_count:,} prices"
    )
    print()

    # Pick user
    users = ds.list_users()
    if not users:
        print("No users found in database. Run 'invoke seed-db' first.")
        sys.exit(1)

    print("Available users:")
    for i, u in enumerate(users, 1):
        email = f" ({u.email})" if u.email else ""
        print(f"  {i:3d}. {u.id}{email} - {u.trade_count} trades")

    print()
    while True:
        choice = input(f"Select user [1-{len(users)}]: ").strip()
        try:
            idx = int(choice) - 1
            if 0 <= idx < len(users):
                selected_user = users[idx]
                break
        except ValueError:
            pass
        print("Invalid selection, try again.")

    # Pick role
    role = input("Role [user/admin] (default: user): ").strip().lower()
    if role not in ("admin", "user"):
        role = "user"

    # Base currency
    base_currency = input("Base currency (default: SEK): ").strip().upper()
    if not base_currency:
        base_currency = "SEK"

    ctx = UserContext(
        user_id=selected_user.id,
        role=role,
        base_currency=base_currency,
        as_of_date=date.today(),
    )

    print()
    print(f"Logged in as {ctx.user_id} ({ctx.role}), base currency: {ctx.base_currency}")
    print("Type your questions, or 'quit' to exit.")
    print("-" * 40)

    # Chat loop
    client = anthropic.Anthropic()
    access_note = (
        "You have admin access and can view any user."
        if ctx.role == "admin"
        else "You can only view this users own portfolio."
    )
    system_prompt = (
        f"You are a financial portfolio analyst assistant for the Calce calculation engine. "
        f'You are currently helping user "{ctx.user_id}" (role: {ctx.role}). '
        f"Base currency: {ctx.base_currency}. Date: {ctx.as_of_date}. "
        f"{access_note} "
        f"Use your tools to answer questions about portfolios and market data. "
        f"Format numbers clearly and provide analytical insights."
    )

    messages: list[dict] = []

    while True:
        try:
            user_input = input("\n> ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nGoodbye!")
            break

        if not user_input:
            continue
        if user_input.lower() in ("quit", "exit"):
            print("Goodbye!")
            break

        messages.append({"role": "user", "content": user_input})

        # Agentic tool-use loop
        while True:
            response = client.messages.create(
                model=MODEL,
                max_tokens=4096,
                system=system_prompt,
                tools=TOOL_DEFINITIONS,
                messages=messages,
            )

            messages.append({"role": "assistant", "content": response.content})

            if response.stop_reason == "tool_use":
                tool_results = []
                for block in response.content:
                    if block.type == "tool_use":
                        print(f"  [calling {block.name}...]")
                        result = execute_tool(ds, ctx, block.name, block.input)
                        tool_results.append(
                            {
                                "type": "tool_result",
                                "tool_use_id": block.id,
                                "content": result,
                            }
                        )
                messages.append({"role": "user", "content": tool_results})
            else:
                # Print text response
                for block in response.content:
                    if hasattr(block, "text"):
                        print(block.text)
                break


if __name__ == "__main__":
    main()
