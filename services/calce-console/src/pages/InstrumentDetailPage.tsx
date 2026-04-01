import { useParams, Link } from 'react-router'
import { useQuery } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { useMemo } from 'react'
import { api } from '../api/client'
import type { Price } from '../api/types'
import { IconChevronLeft } from '../components/icons'
import Badge from '../components/Badge'
import DataTable from '../components/DataTable'
import PriceChart from '../components/PriceChart'
import Spinner from '../components/Spinner'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function InstrumentDetailPage() {
  const { id } = useParams()
  useEntityEvents(['instruments', 'prices'])

  const numericId = Number(id)

  const { data: instrument, isLoading: instrumentLoading, error: instrumentError } = useQuery({
    queryKey: ['instrument', id],
    queryFn: () => api.getInstrument(numericId),
    enabled: Number.isFinite(numericId),
  })

  usePageTitle(instrument?.name ?? 'Instrument')

  const { data: prices, isLoading: pricesLoading } = useQuery({
    queryKey: ['prices', { ticker: instrument?.ticker }],
    queryFn: () => {
      const to = new Date().toISOString().slice(0, 10)
      const from = new Date(Date.now() - 5 * 365 * 24 * 60 * 60 * 1000)
        .toISOString()
        .slice(0, 10)
      return api.getInstrumentPrices(instrument!.ticker, { from, to })
    },
    enabled: !!instrument,
  })

  const priceColumns = useMemo<ColumnDef<Price, unknown>[]>(
    () => [
      {
        accessorKey: 'date',
        header: 'Date',
        cell: ({ getValue }) =>
          new Date(getValue<string>()).toLocaleDateString(),
      },
      {
        accessorKey: 'price',
        header: 'Price',
        meta: { numeric: true },
        cell: ({ getValue }) => (
          <span className="ds-text--mono">
            {getValue<number>().toFixed(2)}
          </span>
        ),
      },
    ],
    []
  )

  if (instrumentLoading) {
    return (
      <div className="ds-page">
        <Spinner size="lg" center />
      </div>
    )
  }

  if (instrumentError || !instrument) {
    return (
      <div className="ds-page">
        <Link to="/instruments" className="ds-back-link">
          <IconChevronLeft size={12} /> Back to Instruments
        </Link>
        <p className="ds-text--secondary">{instrumentError?.message || 'Instrument not found.'}</p>
      </div>
    )
  }

  return (
    <div className="ds-page">
      <Link to="/instruments" className="ds-back-link">
        <IconChevronLeft size={12} /> Back to Instruments
      </Link>
      <div className="ds-page__header">
        <div className="ds-page__actions">
          <h1 className="ds-page__title">{instrument.name || instrument.ticker}</h1>
          <Badge variant="neutral">{instrument.ticker}</Badge>
          <Badge variant="info">{instrument.instrument_type}</Badge>
        </div>
      </div>

      <div className="ds-kv-inline ds-mt-md">
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">Ticker</span>
          <span className="ds-text--mono">{instrument.ticker}</span>
        </span>
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">Name</span>
          <span>{instrument.name || '-'}</span>
        </span>
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">Type</span>
          <span>{instrument.instrument_type}</span>
        </span>
        <span className="ds-kv-inline__item">
          <span className="ds-kv-inline__label">Currency</span>
          <span>{instrument.currency}</span>
        </span>
      </div>

      <div className="ds-chart-container ds-mt-lg">
        {pricesLoading ? (
          <Spinner size="lg" center />
        ) : prices ? (
          <PriceChart data={prices} />
        ) : null}
      </div>

      {!pricesLoading && prices && (
        <DataTable data={prices} columns={priceColumns} />
      )}
    </div>
  )
}
