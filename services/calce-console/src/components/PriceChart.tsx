import { useEffect, useRef } from 'react'
import { createChart, type IChartApi, ColorType } from 'lightweight-charts'
import type { Price } from '../api/types'

export interface ChartMarker {
  time: string
  position: 'aboveBar' | 'belowBar' | 'inBar'
  color: string
  shape: 'circle' | 'square' | 'arrowUp' | 'arrowDown'
  text?: string
}

interface PriceChartProps {
  data: Price[]
  overlayData?: Price[]
  overlayLabel?: string
  markers?: ChartMarker[]
}

function PriceChart({ data, overlayData, markers }: PriceChartProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const chartRef = useRef<IChartApi | null>(null)

  useEffect(() => {
    if (!containerRef.current || data.length === 0) return

    const chart = createChart(containerRef.current, {
      width: containerRef.current.clientWidth,
      height: 300,
      layout: {
        background: { type: ColorType.Solid, color: 'transparent' },
        textColor: getComputedStyle(document.documentElement).getPropertyValue('--color-text-secondary').trim() || '#9aa0a6',
        fontSize: 11,
      },
      grid: {
        vertLines: { visible: false },
        horzLines: { color: getComputedStyle(document.documentElement).getPropertyValue('--color-border-light').trim() || '#e8eaed' },
      },
      rightPriceScale: {
        borderVisible: false,
      },
      timeScale: {
        borderVisible: false,
      },
    })

    const series = chart.addAreaSeries({
      lineColor: getComputedStyle(document.documentElement).getPropertyValue('--color-primary').trim() || '#1a73e8',
      topColor: 'rgba(26, 115, 232, 0.2)',
      bottomColor: 'rgba(26, 115, 232, 0.02)',
      lineWidth: 2,
    })

    const chartData = data
      .map((p) => ({ time: p.date as string, value: p.price }))
      .sort((a, b) => a.time.localeCompare(b.time))

    series.setData(chartData)

    if (markers && markers.length > 0) {
      const sorted = [...markers].sort((a, b) => a.time.localeCompare(b.time))
      series.setMarkers(sorted)
    }

    if (overlayData && overlayData.length > 0) {
      const overlaySeries = chart.addLineSeries({
        color: getComputedStyle(document.documentElement).getPropertyValue('--color-warning').trim() || '#e8710a',
        lineWidth: 2,
        lineStyle: 2,
      })

      const overlayChartData = overlayData
        .map((p) => ({ time: p.date as string, value: p.price }))
        .sort((a, b) => a.time.localeCompare(b.time))

      overlaySeries.setData(overlayChartData)
    }

    chart.timeScale().fitContent()
    chartRef.current = chart

    const handleResize = () => {
      if (containerRef.current) {
        chart.applyOptions({ width: containerRef.current.clientWidth })
      }
    }

    window.addEventListener('resize', handleResize)
    return () => {
      window.removeEventListener('resize', handleResize)
      chart.remove()
      chartRef.current = null
    }
  }, [data, overlayData, markers])

  if (data.length === 0) {
    return <div style={{ padding: '48px', textAlign: 'center', color: 'var(--color-text-tertiary)' }}>No price data</div>
  }

  return <div ref={containerRef} style={{ width: '100%' }} />
}

export default PriceChart
