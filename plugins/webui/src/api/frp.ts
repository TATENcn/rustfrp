import { apiDelete, apiGet, apiPost, apiPut } from "@/api/client";
import type {
  ApiEnvelope,
  AvailableFrpVersion,
  FrpVersionList,
  InstalledFrpVersion,
} from "@/api/types";

export function listFrpVersions(): Promise<ApiEnvelope<FrpVersionList>> {
  return apiGet<FrpVersionList>("/frp/versions");
}

export function listAvailableFrpVersions(): Promise<
  ApiEnvelope<AvailableFrpVersion[]>
> {
  return apiGet<AvailableFrpVersion[]>("/frp/releases");
}

export function installFrpVersion(
  version: string,
  mirrorBase?: string,
): Promise<ApiEnvelope<InstalledFrpVersion>> {
  return apiPost<InstalledFrpVersion>("/frp/versions", {
    version,
    mirror_base: mirrorBase || null,
  });
}

export function activateFrpVersion(
  version: string,
): Promise<ApiEnvelope<FrpVersionList>> {
  return apiPost<FrpVersionList>(
    `/frp/versions/${encodeURIComponent(version)}/activate`,
  );
}

export function deleteFrpVersion(version: string): Promise<ApiEnvelope<null>> {
  return apiDelete(`/frp/versions/${encodeURIComponent(version)}`);
}

export interface InstallTask {
  id: string;
  version: string;
  source: string;
  status: "running" | "completed" | "failed";
  stage: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  error: string | null;
}
export const getInstallation = () =>
  apiGet<InstallTask | null>("/frp/installations");
export const beginInstallation = (
  version: string,
  channel: string,
  mirror_base?: string,
) =>
  apiPost<InstallTask>("/frp/installations", {
    version,
    channel,
    mirror_base: mirror_base || null,
  });
