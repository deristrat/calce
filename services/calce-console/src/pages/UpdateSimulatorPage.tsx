import { useState, useEffect, useCallback } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { DbSimulatorConfig } from '../api/types'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEventSource } from '../hooks/useEventSource'
import { useEventBuffer } from '../hooks/useEventBuffer'
import Button from '../components/Button'
import Card from '../components/Card'
import StatCard from '../components/StatCard'
import Badge from '../components/Badge'
import Input from '../components/Input'

const INITIAL_COUNTS = { price_current: 0, fx_current: 0 } as const;
type Counts = typeof INITIAL_COUNTS;

function classifyEvent(event: { type: string }): keyof Counts | null {
  if (event.type === 'price') return 'price_current';
  if (event.type === 'fx') return 'fx_current';
  return null;
}

const DEFAULT_CONFIG: DbSimulatorConfig = {
  tick_interval_ms: 500,
  prices_per_tick: 5,
  fx_per_tick: 2,
}

export default function UpdateSimulatorPage() {
  usePageTitle('Update Simulator')
  const queryClient = useQueryClient()

  const [config, setConfig] = useState<DbSimulatorConfig>(DEFAULT_CONFIG)

  const { data: stats, isLoading } = useQuery({
    queryKey: ['db-simulator-status'],
    queryFn: () => api.getDbSimulatorStatus(),
    refetchInterval: (query) => query.state.data?.running ? 1000 : false,
  })

  useEffect(() => {
    if (stats?.config) setConfig(stats.config)
  }, [stats?.config])

  const startMutation = useMutation({
    mutationFn: () => api.startDbSimulator(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['db-simulator-status'] })
    },
  })

  const stopMutation = useMutation({
    mutationFn: () => api.stopDbSimulator(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['db-simulator-status'] })
    },
  })

  const running = stats?.running ?? false
  const busy = startMutation.isPending || stopMutation.isPending
  const mutationError = startMutation.error || stopMutation.error

  // SSE + event buffering
  const { recentEvents, totalEvents, counts, pushEvent } = useEventBuffer<Counts>(
    INITIAL_COUNTS,
    classifyEvent,
  );
  const onEvent = useCallback((data: unknown) => pushEvent(data), [pushEvent]);
  const { connected: sseConnected, connect: connectSse, disconnect: disconnectSse } = useEventSource(
    '/v1/admin/market-events',
    { onEvent },
  );

  function formatNumber(n: number | undefined): string {
    if (n === undefined) return '-'
    return n.toLocaleString()
  }

  function updateConfig(key: keyof DbSimulatorConfig, value: string) {
    const n = parseInt(value, 10)
    if (!isNaN(n) && n >= 0) setConfig(prev => ({ ...prev, [key]: n }))
  }

  return (
    <div className="ds-page">
      <div className="ds-page__header">
        <div>
          <h1 className="ds-page__title">Update Simulator</h1>
          <p className="ds-text--secondary ds-mt-xs">
            Writes prices to the database. CDC propagates changes to the cache.
          </p>
        </div>
        <div className="ds-page__actions">
          <Badge variant={running ? 'success' : 'neutral'}>
            {running ? 'Running' : 'Stopped'}
          </Badge>
          {running ? (
            <Button variant="danger" onClick={() => stopMutation.mutate()} disabled={busy}>
              Stop
            </Button>
          ) : (
            <Button variant="primary" onClick={() => startMutation.mutate()} disabled={busy}>
              Start
            </Button>
          )}
        </div>
      </div>

      {mutationError && (
        <p className="ds-text--error">Error: {mutationError.message}</p>
      )}

      {isLoading ? (
        <p className="ds-text--secondary">Loading...</p>
      ) : stats ? (
        <>
          <div className="ds-grid ds-grid--cols-4 ds-mb-xl">
            <StatCard label="Ticks" value={formatNumber(stats.ticks)} />
            <StatCard label="Price Writes" value={formatNumber(stats.price_writes)} />
            <StatCard label="FX Writes" value={formatNumber(stats.fx_writes)} />
            <StatCard label="Errors" value={formatNumber(stats.errors)} />
          </div>

          <Card>
            <h3 className="ds-mb-sm">Configuration</h3>
            <table className="ds-table">
              <tbody>
                <tr>
                  <td className="ds-table__cell">Tick interval (ms)</td>
                  <td className="ds-table__cell ds-table__cell--numeric">
                    <Input className="ds-input--compact" type="number" min="10" step="50"
                      value={config.tick_interval_ms} disabled={running}
                      onChange={e => updateConfig('tick_interval_ms', e.target.value)} />
                  </td>
                </tr>
                <tr>
                  <td className="ds-table__cell">Price writes per tick</td>
                  <td className="ds-table__cell ds-table__cell--numeric">
                    <Input className="ds-input--compact" type="number" min="0" step="1"
                      value={config.prices_per_tick} disabled={running}
                      onChange={e => updateConfig('prices_per_tick', e.target.value)} />
                  </td>
                </tr>
                <tr>
                  <td className="ds-table__cell">FX writes per tick</td>
                  <td className="ds-table__cell ds-table__cell--numeric">
                    <Input className="ds-input--compact" type="number" min="0" step="1"
                      value={config.fx_per_tick} disabled={running}
                      onChange={e => updateConfig('fx_per_tick', e.target.value)} />
                  </td>
                </tr>
              </tbody>
            </table>
          </Card>

          <Card>
            <div className="ds-flex ds-flex--between ds-flex--center ds-mb-sm">
              <h3>Live CDC Events</h3>
              <div className="ds-page__actions">
                <Badge variant={sseConnected ? 'success' : 'neutral'}>
                  {sseConnected ? 'Connected' : 'Disconnected'}
                </Badge>
                {sseConnected ? (
                  <Button variant="outline" onClick={disconnectSse}>Disconnect</Button>
                ) : (
                  <Button variant="outline" onClick={connectSse}>Connect</Button>
                )}
              </div>
            </div>

            {totalEvents > 0 && (
              <div className="ds-grid ds-grid--cols-3 ds-mb-md">
                <StatCard label="Total Events" value={formatNumber(totalEvents)} />
                <StatCard label="Price (current)" value={formatNumber(counts.price_current)} />
                <StatCard label="FX (current)" value={formatNumber(counts.fx_current)} />
              </div>
            )}

            {recentEvents.length > 0 ? (
              <div className="ds-scroll-y ds-table--sticky-header">
                <table className="ds-table">
                  <thead>
                    <tr>
                      <th className="ds-table__cell">Time</th>
                      <th className="ds-table__cell">Type</th>
                      <th className="ds-table__cell">Key</th>
                      <th className="ds-table__cell">Kind</th>
                    </tr>
                  </thead>
                  <tbody>
                    {recentEvents.map((ev, i) => (
                      <tr key={i}>
                        <td className="ds-table__cell ds-text--mono">{ev.time.slice(11, 23)}</td>
                        <td className="ds-table__cell">
                          <Badge variant={ev.type === 'price' ? 'info' : 'warning'}>{ev.type}</Badge>
                        </td>
                        <td className="ds-table__cell ds-text--mono">{ev.key}</td>
                        <td className="ds-table__cell">{ev.kind}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <p className="ds-text--secondary">
                {sseConnected ? 'Waiting for events... Start the simulator to see CDC updates.' : 'Click Connect to subscribe to live CDC events.'}
              </p>
            )}
          </Card>
        </>
      ) : null}
    </div>
  )
}
