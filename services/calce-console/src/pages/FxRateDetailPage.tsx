import { useParams, Link } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo, useState } from 'react'
import { api } from '../api/client'
import type { Price } from '../api/types'
import { PAGE_SIZE } from '../constants'
import { IconChevronLeft } from '../components/icons'
import Badge from '../components/Badge'
import DataTable from '../components/DataTable'
import Pagination from '../components/Pagination'
import PriceChart from '../components/PriceChart'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

interface MergedRow {
  date: string
  price: number
  reverseRate: number | null
  invertedRate: number | null
}

export default function FxRateDetailPage() {
  const { from, to } = useParams()
  const pair = `${from}/${to}`
  const reversePair = `${to}/${from}`

  usePageTitle(pair)
  useEntityEvents(['fx_rates'])

  const [showReverse, setShowReverse] = useState(false)
  const [page, setPage] = useState(1)

  const dateRange = useMemo(() => {
    const toDate = new Date().toISOString().slice(0, 10)
    const fromDate = new Date(Date.now() - 5 * 365 * 24 * 60 * 60 * 1000)
      .toISOString()
      .slice(0, 10)
    return { from: fromDate, to: toDate }
  }, [])

  const { data: history, isLoading } = useQuery({
    queryKey: ['fx_rates', from, to],
    queryFn: () => api.getFxRateHistory(from!, to!, dateRange),
    enabled: !!from && !!to,
  })

  const { data: reverseHistory } = useQuery({
    queryKey: ['fx_rates', to, from],
    queryFn: () => api.getFxRateHistory(to!, from!, dateRange),
    enabled: !!from && !!to,
  })

  const reverseAvailable = !!reverseHistory && reverseHistory.length > 0

  const invertedReverseData = useMemo<Price[]>(() => {
    if (!reverseHistory) return []
    return reverseHistory.map((p) => ({ date: p.date, price: 1 / p.price }))
  }, [reverseHistory])

  const mergedData = useMemo<MergedRow[]>(() => {
    if (!history) return []
    if (!showReverse || !reverseHistory) {
      return [...history]
        .sort((a, b) => b.date.localeCompare(a.date))
        .map((p) => ({
          date: p.date,
          price: p.price,
          reverseRate: null,
          invertedRate: null,
        }))
    }
    const reverseByDate = new Map(reverseHistory.map((p) => [p.date, p.price]))
    const allDates = new Set([
      ...history.map((p) => p.date),
      ...reverseHistory.map((p) => p.date),
    ])
    const primaryByDate = new Map(history.map((p) => [p.date, p.price]))
    return Array.from(allDates)
      .sort((a, b) => b.localeCompare(a))
      .map((date) => {
        const rev = reverseByDate.get(date) ?? null
        return {
          date,
          price: primaryByDate.get(date) ?? NaN,
          reverseRate: rev,
          invertedRate: rev != null ? 1 / rev : null,
        }
      })
      .filter((r) => !isNaN(r.price))
  }, [history, reverseHistory, showReverse])

  const latestRate = history && history.length > 0 ? history[history.length - 1] : null

  const columns = useMemo<ColumnDef<MergedRow, unknown>[]>(
    () => {
      const cols: ColumnDef<MergedRow, unknown>[] = [
        {
          accessorKey: 'date',
          header: 'Date',
          cell: ({ getValue }) =>
            new Date(getValue<string>()).toLocaleDateString(),
        },
        {
          accessorKey: 'price',
          header: `Rate (${pair})`,
          meta: { numeric: true },
          cell: ({ getValue }) => (
            <span className="ds-text--mono">
              {getValue<number>().toFixed(4)}
            </span>
          ),
        },
      ]
      if (showReverse && reverseAvailable) {
        cols.push(
          {
            accessorKey: 'reverseRate',
            header: `Rate (${reversePair})`,
            meta: { numeric: true },
            cell: ({ getValue }) => {
              const v = getValue<number | null>()
              return v != null ? (
                <span className="ds-text--mono">{v.toFixed(4)}</span>
              ) : '-'
            },
          },
          {
            accessorKey: 'invertedRate',
            header: `1 / ${reversePair}`,
            meta: { numeric: true },
            cell: ({ getValue }) => {
              const v = getValue<number | null>()
              return v != null ? (
                <span className="ds-text--mono">{v.toFixed(4)}</span>
              ) : '-'
            },
          },
        )
      }
      return cols
    },
    [pair, reversePair, showReverse, reverseAvailable]
  )

  return (
    <div className="ds-page">
      <Link to="/fx-rates" className="ds-back-link">
        <IconChevronLeft size={12} /> Back to FX Rates
      </Link>
      <div className="ds-page__header">
        <div className="ds-page__actions">
          <h1 className="ds-page__title">{pair}</h1>
          <Badge variant="neutral">{from}</Badge>
          <Badge variant="neutral">{to}</Badge>
        </div>
      </div>

      <div className="ds-kv-inline ds-mt-md">
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">From</span>
          <span>{from}</span>
        </span>
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">To</span>
          <span>{to}</span>
        </span>
        {latestRate && (
          <>
            <span className="ds-kv-inline__item">
              <span className="ds-kv-inline__label">Latest Rate</span>
              <span className="ds-text--mono">{latestRate.price.toFixed(4)}</span>
            </span>
            <span className="ds-kv-inline__item">
              <span className="ds-kv-inline__label">Latest Date</span>
              <span>{new Date(latestRate.date).toLocaleDateString()}</span>
            </span>
          </>
        )}
        {history && (
          <span className="ds-kv-inline__item">
            <span className="ds-kv-inline__label">Data Points</span>
            <span>{history.length.toLocaleString()}</span>
          </span>
        )}
      </div>

      {reverseAvailable && (
        <div className="ds-kv-inline ds-mt-md">
          <label className="ds-kv-inline__item" style={{ cursor: 'pointer' }}>
            <button
              type="button"
              className={`ds-toggle${showReverse ? ' ds-toggle--checked' : ''}`}
              onClick={() => { setShowReverse(!showReverse); setPage(1) }}
              aria-pressed={showReverse}
            />
            <span>
              Reverse rate <Link to={`/fx-rates/${to}/${from}`}><Badge variant="neutral">{reversePair}</Badge></Link> available — show for comparison
            </span>
          </label>
        </div>
      )}

      <div className="ds-chart-container ds-mt-lg">
        {isLoading ? (
          <Spinner size="lg" center />
        ) : history && history.length > 0 ? (
          <PriceChart
            data={history}
            overlayData={showReverse ? invertedReverseData : undefined}
            overlayLabel={`1/${reversePair}`}
          />
        ) : (
          <p className="ds-text--secondary">No rate history available.</p>
        )}
      </div>

      {!isLoading && mergedData.length > 0 && (() => {
        const totalPages = Math.ceil(mergedData.length / PAGE_SIZE)
        const pageData = mergedData.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE)
        return (
          <>
            <DataTable data={pageData} columns={columns} />
            {totalPages > 1 && (
              <Pagination page={page} totalPages={totalPages} onPageChange={setPage} />
            )}
          </>
        )
      })()}
    </div>
  )
}
