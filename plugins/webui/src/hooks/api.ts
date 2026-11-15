import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { ApiEnvelope } from "@/api/types";
import { AppError, extractApiError } from "@/api/errors";
import { getStatus } from "@/api/system";
import { listProfiles } from "@/api/profiles";
import { listProxies } from "@/api/proxies";
import { listVisitors } from "@/api/visitors";
import { listBindings } from "@/api/bindings";
import { listEnvironments } from "@/api/environments";
import { usePreferences } from "@/hooks/preferences";
export async function data<T>(request: Promise<ApiEnvelope<T>>): Promise<T> {
  const response = await request;
  if (!response.success || response.data === undefined)
    throw new AppError(
      response.error?.message || "Empty API response",
      response.error?.code,
    );
  return response.data;
}
// Commands such as DELETE may succeed without a response payload.
export async function command(request: Promise<ApiEnvelope<unknown>>): Promise<void> {
  const response = await request;
  if (!response.success)
    throw new AppError(
      response.error?.message || "API request failed",
      response.error?.code,
    );
}
/**
 * Resolve the environment selected in the sidebar, falling back to the tenant's
 * default. Every page that scopes data by environment goes through this so the
 * dropdown, the queries, and their cache keys cannot disagree.
 */
export function useActiveEnvironment() {
  const prefs = usePreferences();
  const environments = useEnvironments();
  const environment =
    environments.data?.find((e) => e.id === prefs.environmentId) ??
    environments.data?.find((e) => e.is_default);
  return { environments, environment };
}
export function useStatus() {
  const { environment } = useActiveEnvironment();
  return useQuery({
    queryKey: ["status", environment?.id ?? null],
    queryFn: () => data(getStatus(environment?.id)),
    refetchInterval: (q) =>
      q.state.fetchFailureCount
        ? Math.min(60000, 3000 * 2 ** q.state.fetchFailureCount)
        : 3000,
  });
}
export function useProfiles() {
  return useQuery({
    queryKey: ["profiles"],
    queryFn: () => data(listProfiles()),
  });
}
export function useProxies() {
  return useQuery({
    queryKey: ["proxies"],
    queryFn: () => data(listProxies()),
  });
}
export function useVisitors() {
  return useQuery({
    queryKey: ["visitors"],
    queryFn: () => data(listVisitors()),
  });
}
export function useBindings() {
  return useQuery({
    queryKey: ["bindings"],
    queryFn: () => data(listBindings()),
  });
}
export function useEnvironments() {
  return useQuery({
    queryKey: ["environments"],
    queryFn: () => data(listEnvironments()),
  });
}
export function useAction<T>(
  fn: (value: T) => Promise<unknown>,
  success?: string,
  onSuccess?: () => void,
) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: async () => {
      onSuccess?.();
      if (success) toast.success(success);
      await client.invalidateQueries();
    },
    onError: async (error) => {
      toast.error(extractApiError(error).message);
      // A batch may have committed some records before a later request failed.
      await client.invalidateQueries();
    },
  });
}
