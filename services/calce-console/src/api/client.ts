import type {
  AccountSummary,
  ApiKey,
  ApiKeyCreated,
  DataStats,
  FxRateSummary,
  Instrument,
  LoginResponse,
  Organization,
  PaginatedResponse,
  PositionSummary,
  Price,
  DbSimulatorConfig,
  DbSimulatorStats,
  SimulatorConfig,
  SimulatorStats,
  SystemConfig,
  SystemInfo,
  TradeSummary,
  User,
} from "./types";

const TOKEN_KEY = "access_token";
const REFRESH_KEY = "refresh_token";

let refreshPromise: Promise<void> | null = null;

function buildQuery(params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      search.set(key, String(value));
    }
  }
  const qs = search.toString();
  return qs ? `?${qs}` : "";
}

async function refreshTokens(): Promise<void> {
  const refreshToken = localStorage.getItem(REFRESH_KEY);
  if (!refreshToken) {
    throw new Error("No refresh token");
  }

  console.info("[auth] refreshing token");
  const res = await fetch("/auth/refresh", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ refresh_token: refreshToken }),
  });

  if (!res.ok) {
    console.error("[auth] token refresh failed:", res.status, res.statusText);
    throw new Error("Refresh failed");
  }

  const data: LoginResponse = await res.json();
  localStorage.setItem(TOKEN_KEY, data.access_token);
  localStorage.setItem(REFRESH_KEY, data.refresh_token);
  console.info("[auth] token refreshed");
}

export async function fetchApi<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = localStorage.getItem(TOKEN_KEY);
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options.headers as Record<string, string>),
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  let res = await fetch(path, { ...options, headers });

  if (res.status === 401 && token) {
    try {
      if (!refreshPromise) {
        refreshPromise = refreshTokens();
      }
      await refreshPromise;
      refreshPromise = null;

      const newToken = localStorage.getItem(TOKEN_KEY);
      headers["Authorization"] = `Bearer ${newToken}`;
      res = await fetch(path, { ...options, headers });
    } catch (err) {
      refreshPromise = null;
      console.error("[auth] session expired, redirecting to login", err);
      localStorage.removeItem(TOKEN_KEY);
      localStorage.removeItem(REFRESH_KEY);
      window.location.href = "/login";
      throw new Error("Session expired");
    }
  }

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: "unknown", message: res.statusText }));
    const msg = body.message || res.statusText;
    console.error(`[api] ${options.method ?? "GET"} ${path} → ${res.status}: ${msg}`);
    throw new Error(msg);
  }

  return res.json();
}

export const api = {
  async login(email: string, password: string): Promise<void> {
    const data = await fetchApi<LoginResponse>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    });
    localStorage.setItem(TOKEN_KEY, data.access_token);
    localStorage.setItem(REFRESH_KEY, data.refresh_token);
  },

  logout(): void {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(REFRESH_KEY);
  },

  getAccessToken(): string | null {
    return localStorage.getItem(TOKEN_KEY);
  },

  isLoggedIn(): boolean {
    return localStorage.getItem(TOKEN_KEY) !== null;
  },

  getStats(): Promise<DataStats> {
    return fetchApi<DataStats>("/v1/data/stats");
  },

  getInstruments(params: {
    offset?: number;
    limit?: number;
    search?: string;
  }): Promise<PaginatedResponse<Instrument>> {
    const qs = buildQuery(params);
    return fetchApi<PaginatedResponse<Instrument>>(`/v1/data/instruments${qs}`);
  },

  getInstrument(id: number): Promise<Instrument> {
    return fetchApi<Instrument>(`/v1/data/instruments/${id}`);
  },

  getInstrumentPrices(
    instrumentId: string,
    params?: { from?: string; to?: string },
  ): Promise<Price[]> {
    const qs = buildQuery(params ?? {});
    return fetchApi<Price[]>(`/v1/data/instruments/${instrumentId}/prices${qs}`);
  },

  getUser(id: string): Promise<User> {
    return fetchApi<User>(`/v1/data/users/${id}`);
  },

  getUsers(params: {
    offset?: number;
    limit?: number;
    search?: string;
    organization_id?: string;
  }): Promise<PaginatedResponse<User>> {
    const qs = buildQuery(params);
    return fetchApi<PaginatedResponse<User>>(`/v1/data/users${qs}`);
  },

  getUserAccounts(userId: string): Promise<AccountSummary[]> {
    return fetchApi<AccountSummary[]>(`/v1/data/users/${userId}/accounts`);
  },

  getUserPositions(userId: string): Promise<PositionSummary[]> {
    return fetchApi<PositionSummary[]>(`/v1/data/users/${userId}/positions`);
  },

  getUserTrades(userId: string): Promise<TradeSummary[]> {
    return fetchApi<TradeSummary[]>(`/v1/data/users/${userId}/trades`);
  },

  getUserPositionTrades(userId: string, instrumentId: string): Promise<TradeSummary[]> {
    return fetchApi<TradeSummary[]>(
      `/v1/data/users/${userId}/positions/${encodeURIComponent(instrumentId)}/trades`,
    );
  },

  getAccountPositions(userId: string, accountId: number): Promise<PositionSummary[]> {
    return fetchApi<PositionSummary[]>(
      `/v1/data/users/${userId}/accounts/${accountId}/positions`,
    );
  },

  getAccountTrades(userId: string, accountId: number): Promise<TradeSummary[]> {
    return fetchApi<TradeSummary[]>(
      `/v1/data/users/${userId}/accounts/${accountId}/trades`,
    );
  },

  getFxRates(params: {
    offset?: number;
    limit?: number;
    search?: string;
    from_currency?: string;
    to_currency?: string;
  }): Promise<PaginatedResponse<FxRateSummary>> {
    const qs = buildQuery(params);
    return fetchApi<PaginatedResponse<FxRateSummary>>(`/v1/data/fx-rates${qs}`);
  },

  getFxRateHistory(
    from: string,
    to: string,
    params: { from?: string; to?: string },
  ): Promise<Price[]> {
    const qs = buildQuery(params);
    return fetchApi<Price[]>(`/v1/data/fx-rates/${from}/${to}/history${qs}`);
  },

  updateUser(
    id: string,
    body: { name?: string; email?: string },
  ): Promise<User> {
    return fetchApi<User>(`/v1/users/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  },

  deleteUser(id: string): Promise<void> {
    return fetchApi<void>(`/v1/users/${id}`, { method: "DELETE" });
  },

  getOrganizations(): Promise<Organization[]> {
    return fetchApi<Organization[]>("/v1/organizations");
  },

  getOrganization(orgId: string): Promise<Organization> {
    return fetchApi<Organization>(`/v1/organizations/${orgId}`);
  },

  getApiKeys(orgId: string): Promise<{ items: ApiKey[] }> {
    return fetchApi<{ items: ApiKey[] }>(`/v1/organizations/${orgId}/api-keys`);
  },

  createApiKey(orgId: string, body: { name: string; expires_at?: string }): Promise<ApiKeyCreated> {
    return fetchApi<ApiKeyCreated>(`/v1/organizations/${orgId}/api-keys`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  },

  revokeApiKey(orgId: string, keyId: number): Promise<void> {
    return fetchApi<void>(`/v1/organizations/${orgId}/api-keys/${keyId}`, {
      method: "DELETE",
    });
  },

  getSimulatorStatus(): Promise<SimulatorStats> {
    return fetchApi<SimulatorStats>("/v1/admin/simulator/status");
  },

  startSimulator(config: SimulatorConfig): Promise<SimulatorStats> {
    return fetchApi<SimulatorStats>("/v1/admin/simulator/start", {
      method: "POST",
      body: JSON.stringify(config),
    });
  },

  stopSimulator(): Promise<SimulatorStats> {
    return fetchApi<SimulatorStats>("/v1/admin/simulator/stop", {
      method: "POST",
    });
  },

  getDbSimulatorStatus(): Promise<DbSimulatorStats> {
    return fetchApi<DbSimulatorStats>("/v1/admin/db-simulator/status");
  },

  startDbSimulator(config: DbSimulatorConfig): Promise<DbSimulatorStats> {
    return fetchApi<DbSimulatorStats>("/v1/admin/db-simulator/start", {
      method: "POST",
      body: JSON.stringify(config),
    });
  },

  stopDbSimulator(): Promise<DbSimulatorStats> {
    return fetchApi<DbSimulatorStats>("/v1/admin/db-simulator/stop", {
      method: "POST",
    });
  },

  getSystemInfo(): Promise<SystemInfo> {
    return fetchApi<SystemInfo>("/v1/admin/system/info");
  },

  getSystemConfig(): Promise<SystemConfig> {
    return fetchApi<SystemConfig>("/v1/admin/system/config");
  },
};
