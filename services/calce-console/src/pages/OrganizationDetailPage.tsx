import { useMemo, useState } from 'react'
import { useParams, Link } from 'react-router'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { ColumnDef } from '@tanstack/react-table'
import { api } from '../api/client'
import type { ApiKeyCreated } from '../api/types'
import { IconChevronLeft } from '../components/icons'
import Card from '../components/Card'
import Spinner from '../components/Spinner'
import Badge from '../components/Badge'
import Button from '../components/Button'
import Input from '../components/Input'
import Modal from '../components/Modal'
import DataTable from '../components/DataTable'
import { usePageTitle } from '../hooks/usePageTitle'
import { useEntityEvents } from '../hooks/useEntityEvents'

type ApiKeyRow = {
  id: number
  name: string
  key_prefix: string
  expires_at: string | null
  created_at: string
}

export default function OrganizationDetailPage() {
  const { id } = useParams()
  const queryClient = useQueryClient()
  useEntityEvents(['organizations', 'api_keys'])

  const { data: org, isLoading, error } = useQuery({
    queryKey: ['organization', id],
    queryFn: () => api.getOrganization(id!),
    enabled: !!id,
  })

  const { data: apiKeysData, isLoading: keysLoading } = useQuery({
    queryKey: ['api_keys', id],
    queryFn: () => api.getApiKeys(id!),
    enabled: !!id,
  })

  usePageTitle(org?.name || id || 'Organization')

  if (isLoading) {
    return (
      <div className="ds-page">
        <Spinner size="lg" center />
      </div>
    )
  }

  if (error || !org) {
    return (
      <div className="ds-page">
        <Link to="/organizations" className="ds-back-link">
          <IconChevronLeft size={12} /> Back to Organizations
        </Link>
        <p className="ds-text--secondary">{error?.message || 'Organization not found.'}</p>
      </div>
    )
  }

  const apiKeys = apiKeysData?.items ?? []

  return (
    <div className="ds-page">
      <Link to="/organizations" className="ds-back-link">
        <IconChevronLeft size={12} /> Back to Organizations
      </Link>
      <div className="ds-page__header">
        <h1 className="ds-page__title">{org.name || org.id}</h1>
      </div>

      <Card header="Organization Details">
        <div className="ds-kv-grid">
          <span className="ds-kv-grid__label">ID</span>
          <span className="ds-text--mono">{org.id}</span>
          <span className="ds-kv-grid__label">Name</span>
          <span>{org.name || '-'}</span>
          <span className="ds-kv-grid__label">Users</span>
          <span>
            <Link to={`/users?organization_id=${org.id}`} className="ds-link">
              {org.user_count}
            </Link>
          </span>
          <span className="ds-kv-grid__label">Created</span>
          <span>{new Date(org.created_at).toLocaleDateString()}</span>
        </div>
      </Card>

      <ApiKeysSection orgId={org.id} apiKeys={apiKeys} isLoading={keysLoading} queryClient={queryClient} />
    </div>
  )
}

function ApiKeysSection({
  orgId,
  apiKeys,
  isLoading,
  queryClient,
}: {
  orgId: string
  apiKeys: ApiKeyRow[]
  isLoading: boolean
  queryClient: ReturnType<typeof useQueryClient>
}) {
  const [showCreate, setShowCreate] = useState(false)
  const [createdKey, setCreatedKey] = useState<ApiKeyCreated | null>(null)
  const [revokeTarget, setRevokeTarget] = useState<{ id: number; name: string } | null>(null)

  const revokeMutation = useMutation({
    mutationFn: (keyId: number) => api.revokeApiKey(orgId, keyId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['api_keys', orgId] })
      setRevokeTarget(null)
    },
  })

  const columns = useMemo<ColumnDef<ApiKeyRow, unknown>[]>(
    () => [
      { accessorKey: 'name', header: 'Name' },
      {
        accessorKey: 'key_prefix',
        header: 'Prefix',
        cell: ({ getValue }) => <span className="ds-text--mono">{getValue<string>()}...</span>,
      },
      {
        accessorKey: 'created_at',
        header: 'Created',
        cell: ({ getValue }) => new Date(getValue<string>()).toLocaleDateString(),
      },
      {
        accessorKey: 'expires_at',
        header: 'Expires',
        cell: ({ getValue }) => {
          const expires = getValue<string | null>()
          if (!expires) return <Badge variant="neutral">Never</Badge>
          if (new Date(expires) < new Date()) return <Badge variant="error">Expired</Badge>
          return new Date(expires).toLocaleDateString()
        },
      },
      {
        id: 'actions',
        header: '',
        enableSorting: false,
        cell: ({ row }) => (
          <Button
            variant="danger"
            size="sm"
            onClick={() => setRevokeTarget({ id: row.original.id, name: row.original.name })}
          >
            Revoke
          </Button>
        ),
      },
    ],
    []
  )

  return (
    <>
      <Card
        header={
          <>
            <span>API Keys</span>
            <Button variant="outline" size="sm" onClick={() => setShowCreate(true)}>
              Create Key
            </Button>
          </>
        }
      >
        {isLoading ? (
          <Spinner size="sm" center />
        ) : apiKeys.length === 0 ? (
          <p className="ds-text--secondary">No API keys.</p>
        ) : (
          <DataTable data={apiKeys} columns={columns} />
        )}
      </Card>

      <CreateApiKeyModal
        open={showCreate}
        onClose={() => setShowCreate(false)}
        orgId={orgId}
        queryClient={queryClient}
        onCreated={setCreatedKey}
      />

      <Modal
        open={!!createdKey}
        onClose={() => setCreatedKey(null)}
        title="API Key Created"
        footer={<Button onClick={() => setCreatedKey(null)}>Done</Button>}
      >
        <p className="ds-text--secondary ds-mb-md">
          Copy this key now — it won't be shown again.
        </p>
        <code className="ds-code-block">{createdKey?.key}</code>
      </Modal>

      <Modal
        open={!!revokeTarget}
        onClose={() => setRevokeTarget(null)}
        title="Revoke API Key"
        footer={
          <div className="ds-flex ds-flex--gap-2">
            <Button variant="outline" onClick={() => setRevokeTarget(null)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={() => revokeTarget && revokeMutation.mutate(revokeTarget.id)}
              disabled={revokeMutation.isPending}
            >
              {revokeMutation.isPending ? 'Revoking...' : 'Revoke'}
            </Button>
          </div>
        }
      >
        <p>Are you sure you want to revoke the API key <strong>{revokeTarget?.name}</strong>? This cannot be undone.</p>
      </Modal>
    </>
  )
}

function CreateApiKeyModal({
  open,
  onClose,
  orgId,
  queryClient,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  orgId: string
  queryClient: ReturnType<typeof useQueryClient>
  onCreated: (key: ApiKeyCreated) => void
}) {
  const [name, setName] = useState('')
  const [expiresAt, setExpiresAt] = useState('')

  const createMutation = useMutation({
    mutationFn: () =>
      api.createApiKey(orgId, {
        name,
        expires_at: expiresAt ? new Date(expiresAt + 'T23:59:59Z').toISOString() : undefined,
      }),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['api_keys', orgId] })
      setName('')
      setExpiresAt('')
      onClose()
      onCreated(data)
    },
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name.trim()) createMutation.mutate()
  }

  const todayStr = new Date().toISOString().split('T')[0]

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Create API Key"
      footer={
        <div className="ds-flex ds-flex--gap-2">
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button onClick={() => { if (name.trim()) createMutation.mutate() }} disabled={!name.trim() || createMutation.isPending}>
            {createMutation.isPending ? 'Creating...' : 'Create'}
          </Button>
        </div>
      }
    >
      <form onSubmit={handleSubmit}>
        <div className="ds-form-group">
          <label className="ds-label">Name</label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Production API"
            autoFocus
          />
        </div>
        <div className="ds-form-group">
          <label className="ds-label">Expiry date (optional)</label>
          <Input
            type="date"
            value={expiresAt}
            onChange={(e) => setExpiresAt(e.target.value)}
            min={todayStr}
          />
        </div>
        {createMutation.error && (
          <p className="ds-text--error">{createMutation.error.message}</p>
        )}
      </form>
    </Modal>
  )
}
