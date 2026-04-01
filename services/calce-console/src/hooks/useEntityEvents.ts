import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useAuth } from "../auth/AuthContext";
import { useEventSource } from "./useEventSource";
import type { EntityChangeEvent } from "../api/types";

interface UseEntityEventsOptions {
  /** Additional query key prefixes to invalidate when any watched event arrives. */
  alsoInvalidate?: string[];
}

/**
 * Subscribe to entity change events via SSE and invalidate TanStack Query
 * caches when relevant entities change.
 *
 * @param tables - The table names to react to (e.g. `["users"]`).
 *                 Pass an empty array to react to all tables.
 * @param options - Optional extra invalidation config.
 */
export function useEntityEvents(
  tables: string[],
  options?: UseEntityEventsOptions,
) {
  const { user } = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = user?.role === "admin";

  const { connected } = useEventSource("/v1/events", {
    enabled: isAdmin,
    onEvent: (data) => {
      const event = data as EntityChangeEvent;
      if (tables.length === 0 || tables.includes(event.table)) {
        // Invalidate all queries that match the table name as a query key prefix.
        queryClient.invalidateQueries({ queryKey: [event.table] });
        // Also invalidate singular form (e.g. "user" for "users" table).
        if (event.table.endsWith("s")) {
          queryClient.invalidateQueries({
            queryKey: [event.table.slice(0, -1), event.id],
          });
        }
        // Invalidate any additional query key prefixes requested by the caller.
        if (options?.alsoInvalidate) {
          for (const prefix of options.alsoInvalidate) {
            queryClient.invalidateQueries({ queryKey: [prefix] });
          }
        }
      }
    },
  });

  // Log connection state changes in development.
  useEffect(() => {
    if (isAdmin && connected) {
      console.info("[entity-events] connected, watching:", tables.length ? tables : "all");
    }
  }, [connected, isAdmin, tables]);
}
