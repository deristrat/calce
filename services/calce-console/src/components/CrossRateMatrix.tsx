import { useNavigate } from 'react-router'
import type { FxRateSummary } from '../api/types'

interface CrossRateMatrixProps {
  rates: FxRateSummary[]
}

function CrossRateMatrix({ rates }: CrossRateMatrixProps) {
  const navigate = useNavigate()

  // Build lookup: from -> to -> rate
  const rateMap = new Map<string, Map<string, number>>()
  const currencies = new Set<string>()

  for (const r of rates) {
    currencies.add(r.from_currency)
    currencies.add(r.to_currency)
    if (!rateMap.has(r.from_currency)) rateMap.set(r.from_currency, new Map())
    if (r.latest_rate != null) {
      rateMap.get(r.from_currency)!.set(r.to_currency, r.latest_rate)
    }
  }

  const sorted = [...currencies].sort()

  return (
    <div className="ds-matrix">
      <table>
        <thead>
          <tr>
            <th />
            {sorted.map((to) => (
              <th key={to}>{to}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((from) => (
            <tr key={from}>
              <td>{from}</td>
              {sorted.map((to) => {
                if (from === to) {
                  return (
                    <td key={to} className="ds-matrix__cell--identity">
                      1.0000
                    </td>
                  )
                }
                const rate = rateMap.get(from)?.get(to)
                if (rate == null) {
                  return (
                    <td key={to} className="ds-matrix__cell--empty">
                      &mdash;
                    </td>
                  )
                }
                return (
                  <td
                    key={to}
                    className="ds-matrix__cell--clickable"
                    onClick={() => navigate(`/fx-rates/${from}/${to}`)}
                  >
                    {rate.toFixed(4)}
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default CrossRateMatrix
