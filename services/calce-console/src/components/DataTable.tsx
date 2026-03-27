import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  flexRender,
  type ColumnDef,
  type SortingState,
  type RowData,
} from '@tanstack/react-table'
import { useState } from 'react'
import { IconSort, IconSortAsc, IconSortDesc } from './icons'

declare module '@tanstack/react-table' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    numeric?: boolean
  }
}

interface DataTableProps<T> {
  data: T[]
  columns: ColumnDef<T, unknown>[]
  onRowClick?: (row: T) => void
}

function DataTable<T>({ data, columns, onRowClick }: DataTableProps<T>) {
  const [sorting, setSorting] = useState<SortingState>([])

  const table = useReactTable({
    data,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  })

  return (
    <div className="ds-table-wrap">
      <table className="ds-table">
        <thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => {
                const isNumeric = header.column.columnDef.meta?.numeric
                return (
                  <th
                    key={header.id}
                    className={isNumeric ? 'ds-table__cell--numeric' : undefined}
                    onClick={header.column.getToggleSortingHandler()}
                    style={{ cursor: header.column.getCanSort() ? 'pointer' : 'default' }}
                  >
                    {flexRender(header.column.columnDef.header, header.getContext())}
                    {header.column.getCanSort() && (
                      <span style={{ display: 'inline-flex', marginLeft: 4, verticalAlign: 'middle' }}>
                        {header.column.getIsSorted() === 'asc' ? (
                          <IconSortAsc size={12} />
                        ) : header.column.getIsSorted() === 'desc' ? (
                          <IconSortDesc size={12} />
                        ) : (
                          <IconSort size={12} />
                        )}
                      </span>
                    )}
                  </th>
                )
              })}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              className={onRowClick ? 'ds-table--hoverable' : undefined}
              onClick={() => onRowClick?.(row.original)}
              style={onRowClick ? { cursor: 'pointer' } : undefined}
            >
              {row.getVisibleCells().map((cell) => {
                const isNumeric = cell.column.columnDef.meta?.numeric
                return (
                  <td key={cell.id} className={isNumeric ? 'ds-table__cell--numeric' : undefined}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
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

export default DataTable
