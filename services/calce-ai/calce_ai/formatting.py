from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .tools import UserContext


def format_portfolio_report(report) -> str:
    lines = []
    mv = report.market_value
    lines.append(f"Total Market Value: {_money(mv.total)}")
    lines.append("")

    # Positions
    lines.append(f"Positions ({len(mv.positions)}):")
    for p in mv.positions:
        lines.append(
            f"  {p.instrument_id}: {p.quantity:,.0f} @ {p.price:,.2f} {p.currency.code}"
            f" = {_money(p.market_value_base)}"
        )
    lines.append("")

    # Value changes
    vc = report.value_changes
    lines.append("Value Changes:")
    lines.append(f"  Daily:  {_change(vc.daily)}")
    lines.append(f"  Weekly: {_change(vc.weekly)}")
    lines.append(f"  YTD:    {_change(vc.ytd)}")
    lines.append(f"  Yearly: {_change(vc.yearly)}")
    lines.append("")

    # Type allocation
    ta = report.type_allocation
    if ta.entries:
        lines.append("Type Allocation:")
        for e in ta.entries:
            lines.append(f"  {e.instrument_type}: {e.weight:.1%} ({_money(e.market_value)})")
        lines.append("")

    # Sector allocation
    sa = report.sector_allocation
    if sa.entries:
        lines.append(f"Sector Allocation ({sa.dimension}):")
        for e in sa.entries:
            lines.append(f"  {e.key}: {e.weight:.1%} ({_money(e.market_value)})")

    return "\n".join(lines)


def format_market_value(result) -> str:
    lines = [f"Total Market Value: {_money(result.total)}", ""]
    lines.append(f"Positions ({len(result.positions)}):")
    for p in result.positions:
        lines.append(
            f"  {p.instrument_id}: {p.quantity:,.0f} @ {p.price:,.2f} {p.currency.code}"
            f" = {_money(p.market_value_base)}"
        )
    return "\n".join(lines)


def format_volatility(result, instrument_id: str) -> str:
    return (
        f"Volatility for {instrument_id}:\n"
        f"  Annualized: {result.annualized_volatility:.2%}\n"
        f"  Daily:      {result.daily_volatility:.4%}\n"
        f"  Period:     {result.start_date} to {result.end_date}\n"
        f"  Observations: {result.num_observations}"
    )


def format_instruments(instruments: list) -> str:
    if not instruments:
        return "No instruments found."
    lines = [f"Found {len(instruments)} instrument(s):", ""]
    for i in instruments:
        name = i.name or ""
        lines.append(f"  {i.id:12s} {name:30s} {i.instrument_type:15s} {i.currency}")
    return "\n".join(lines)


def format_price_history(points: list, instrument_id: str) -> str:
    if not points:
        return f"No price data found for {instrument_id}."

    prices = [p.price for p in points]
    latest = points[-1]
    first = points[0]
    change = latest.price - first.price
    change_pct = (change / first.price * 100) if first.price else 0

    lines = [
        f"Price history for {instrument_id} ({len(points)} data points):",
        f"  Period: {first.date} to {latest.date}",
        f"  Latest: {latest.price:,.2f}",
        f"  Min:    {min(prices):,.2f}",
        f"  Max:    {max(prices):,.2f}",
        f"  Change: {change:+,.2f} ({change_pct:+.2f}%)",
        "",
    ]

    # Show up to 20 evenly spaced data points
    step = max(1, len(points) // 20)
    for p in points[::step]:
        lines.append(f"  {p.date}  {p.price:,.2f}")
    if points[-1] not in points[::step]:
        lines.append(f"  {points[-1].date}  {points[-1].price:,.2f}")

    return "\n".join(lines)


def format_data_stats(stats, ctx: UserContext) -> str:
    return (
        f"Data Overview:\n"
        f"  Users:       {stats.user_count:,}\n"
        f"  Instruments: {stats.instrument_count:,}\n"
        f"  Trades:      {stats.trade_count:,}\n"
        f"  Prices:      {stats.price_count:,}\n"
        f"  FX Rates:    {stats.fx_rate_count:,}\n"
        f"\n"
        f"Current session:\n"
        f"  User: {ctx.user_id} (role: {ctx.role})\n"
        f"  Base currency: {ctx.base_currency}\n"
        f"  As-of date: {ctx.as_of_date}"
    )


def _money(m) -> str:
    return f"{m.amount:,.2f} {m.currency.code}"


def _change(vc) -> str:
    pct = f" ({vc.change_pct:+.2f}%)" if vc.change_pct is not None else ""
    return f"{vc.change.amount:+,.2f} {vc.change.currency.code}{pct}"
