export interface Organization {
  id: string;
  name: string | null;
  created_at: string;
  user_count: number;
}

export interface User {
  id: string;
  email: string | null;
  name: string | null;
  organization_id: string | null;
  organization_name: string | null;
  trade_count: number;
  account_count: number;
}

export interface Instrument {
  id: number;
  ticker: string;
  name: string | null;
  instrument_type: string;
  currency: string;
  allocations?: {
    sector?: [string, number][];
  };
}

export interface Price {
  date: string;
  price: number;
}

export interface AccountSummary {
  id: number;
  label: string;
  currency: string;
  trade_count: number;
  position_count: number;
  market_value: number | null;
}

export interface PositionSummary {
  instrument_id: string;
  instrument_name: string | null;
  quantity: number;
  currency: string;
  trade_count: number;
}

export interface TradeSummary {
  account_id: number;
  account_name: string | null;
  instrument_id: string;
  quantity: number;
  price: number;
  total_value: number;
  currency: string;
  date: string;
}

export interface FxRateSummary {
  from_currency: string;
  to_currency: string;
  pair: string;
  data_points: number;
  latest_rate: number | null;
}

export interface DataStats {
  user_count: number;
  organization_count: number;
  instrument_count: number;
  trade_count: number;
  price_count: number;
  fx_rate_count: number;
}

export interface PaginatedResponse<T> {
  items: T[];
  total: number;
}

export interface LoginResponse {
  access_token: string;
  refresh_token: string;
}

export interface ApiKey {
  id: number;
  name: string;
  key_prefix: string;
  expires_at: string | null;
  revoked_at: string | null;
  created_at: string;
}

export interface ApiKeyCreated {
  id: number;
  name: string;
  key: string;
  key_prefix: string;
  expires_at: string | null;
}

export interface ApiErrorResponse {
  error: string;
  message: string;
}
