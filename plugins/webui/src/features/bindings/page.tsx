import { ProfileVersionButton } from "../resources/profile-version";
import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  getProfileRuntime,
  replaceProfileProxies,
  startProfile,
  stopProfile,
} from "@/api/profiles";
import type { FrpsProfile, LocalProxy } from "@/api/types";
import {
  data,
  useAction,
  useBindings,
  useProfiles,
  useProxies,
  useEnvironments,
} from "@/hooks/api";
import { usePreferences } from "@/hooks/preferences";
import { useI18n } from "@/i18n";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "@/components/ui/card";
import { EditorDialog } from "@/components/common/dialogs";
import {
  ActionButton,
  EmptyState,
  ErrorState,
  PageHeading,
  PageLoading,
  RuntimeBadge,
} from "@/components/common/page";
export default function Bindings() {
  const { t, text } = useI18n(),
    profiles = useProfiles(),
    proxies = useProxies(),
    bindings = useBindings(),
    environments = useEnvironments(),
    prefs = usePreferences();
  const [active, setActive] = useState<FrpsProfile | null>(null),
    [selected, setSelected] = useState<number[]>([]),
    [search, setSearch] = useState("");
  const save = useAction(
    () => data(replaceProfileProxies(active!.id!, selected)),
    t("binding.assignmentSaved"),
    () => setActive(null),
  );
  const assigned = (id: number) =>
    (bindings.data ?? [])
      .filter((b) => b.profile_id === id && b.enabled)
      .map((b) => b.proxy_id);
  const environment =
    environments.data?.find((e) => e.id === prefs.environmentId) ??
    environments.data?.find((e) => e.is_default);
  const visible = (profiles.data ?? []).filter(
    (p) => !environment || environment.profile_ids.includes(p.id!),
  );
  const filtered = (proxies.data ?? []).filter((p) =>
    `${p.name} ${p.proxy_type} ${p.local_ip} ${p.local_port}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  return (
    <div className="space-y-6">
      <PageHeading
        title={t("binding.assignments")}
        description={text(
          "将代理分配到服务器，并管理每个连接的运行状态。",
          "Assign proxies to servers and control each connection.",
        )}
      >
        <Button variant="outline" asChild>
          <Link to="/proxies">
            <AppIcon name="proxies" />
            {t("nav.proxies")}
          </Link>
        </Button>
      </PageHeading>
      <ErrorState
        error={
          profiles.error ||
          proxies.error ||
          bindings.error ||
          environments.error
        }
        retry={() => {
          void profiles.refetch();
          void proxies.refetch();
          void bindings.refetch();
          void environments.refetch();
        }}
      />
      {profiles.isPending || proxies.isPending || bindings.isPending ? (
        <PageLoading />
      ) : !visible.length ? (
        <div className="rounded-lg border">
          <EmptyState
            icon="profiles"
            title={t("binding.noProfiles")}
            description={text(
              "先添加一个服务器，再将本地代理分配给它。",
              "Add a server, then assign your local proxies to it.",
            )}
          >
            <Button asChild>
              <Link to="/profiles/new">
                <AppIcon name="plus" />
                {t("profile.create")}
              </Link>
            </Button>
          </EmptyState>
        </div>
      ) : (
        <div className="grid items-start gap-5 lg:grid-cols-2">
          {visible.map((profile) => (
            <Assignment
              key={profile.id}
              profile={profile}
              proxies={(proxies.data ?? []).filter((p) =>
                assigned(profile.id!).includes(p.id!),
              )}
              onConfigure={() => {
                setActive(profile);
                setSelected(assigned(profile.id!));
                setSearch("");
              }}
            />
          ))}
        </div>
      )}
      <EditorDialog
        open={!!active}
        title={t("binding.configureTitle", { name: active?.name ?? "" })}
        description={text(
          "勾选需要在该服务器发布的代理。",
          "Choose the proxies to publish through this server.",
        )}
        busy={save.isPending}
        onClose={() => setActive(null)}
      >
        <div className="relative">
          <AppIcon
            name="search"
            className="absolute left-3 top-2.5 text-muted-foreground"
          />
          <Input
            className="pl-9"
            aria-label={t("binding.searchProxies")}
            placeholder={t("binding.searchProxies")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="max-h-80 overflow-auto rounded-lg border">
          {filtered.map((proxy) => (
            <label
              key={proxy.id}
              className="flex cursor-pointer items-center gap-3 border-b p-4 last:border-0 hover:bg-muted/50"
            >
              <Checkbox
                disabled={save.isPending}
                checked={selected.includes(proxy.id!)}
                onCheckedChange={(checked) =>
                  setSelected((old) =>
                    checked
                      ? [...old, proxy.id!]
                      : old.filter((id) => id !== proxy.id),
                  )
                }
              />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{proxy.name}</p>
                <p className="mt-1 font-mono text-xs text-muted-foreground">
                  {proxy.local_ip}:{proxy.local_port}
                </p>
              </div>
              <Badge variant="outline">{proxy.proxy_type.toUpperCase()}</Badge>
            </label>
          ))}
          {!filtered.length && (
            <EmptyState title={t("binding.noMatchingProxies")} />
          )}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
          <p className="text-sm text-muted-foreground">
            {t("binding.selectedCount", { count: selected.length })}
          </p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              disabled={save.isPending}
              onClick={() => setActive(null)}
            >
              {t("common.cancel")}
            </Button>
            <ActionButton busy={save.isPending} onClick={() => save.mutate()}>
              {t("common.save")}
            </ActionButton>
          </div>
        </div>
      </EditorDialog>
    </div>
  );
}
function Assignment({
  profile,
  proxies,
  onConfigure,
}: {
  profile: FrpsProfile;
  proxies: LocalProxy[];
  onConfigure: () => void;
}) {
  const { t, text } = useI18n();
  const runtime = useQuery({
    queryKey: ["runtime", profile.id],
    queryFn: () => data(getProfileRuntime(profile.id!)),
    refetchInterval: 5000,
  });
  const action = useAction((kind: "start" | "stop") =>
    data(
      kind === "start" ? startProfile(profile.id!) : stopProfile(profile.id!),
    ),
  );
  const state = runtime.data;
  const label = runtime.isPending
    ? text("正在连接", "Connecting")
    : runtime.error
      ? text("状态未知", "Unknown")
      : state?.running
        ? t("binding.profileRunning")
        : state?.desired_running
          ? t("binding.profileOffline")
          : !proxies.length
            ? t("binding.profileUnconfigured")
            : t("binding.profileStoppedState");
  return (
    <Card className="gap-0 overflow-hidden shadow-none">
      <CardHeader className="pb-5">
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-2">
            <CardTitle className="text-base">{profile.name}</CardTitle>
            <ProfileVersionButton
              id={profile.id!}
              name={profile.name}
              readonly
            />
            <CardDescription className="font-mono text-xs">
              {profile.server_addr}:{profile.server_port}
            </CardDescription>
          </div>
          <RuntimeBadge running={state?.running}>{label}</RuntimeBadge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4 pb-5">
        <ErrorState
          error={runtime.error}
          retry={() => void runtime.refetch()}
        />
        {state?.config_pending && (
          <Alert role="status">
            <AppIcon name="info" />
            <AlertTitle>{t("binding.updateRequired")}</AlertTitle>
            <AlertDescription>
              {t("binding.updateRequiredDescription")}
            </AlertDescription>
          </Alert>
        )}
        <div className="flex items-center justify-between border-t pt-4">
          <span className="text-sm text-muted-foreground">
            {t("binding.assignedProxies")}
          </span>
          <Badge variant="secondary">{proxies.length}</Badge>
        </div>
        <div className="space-y-2">
          {proxies.length ? (
            proxies.map((proxy) => (
              <div
                key={proxy.id}
                className="flex items-center justify-between gap-4 rounded-md bg-muted/50 px-3 py-2.5"
              >
                <div className="flex min-w-0 items-center gap-2">
                  <AppIcon
                    name="proxies"
                    className="shrink-0 text-muted-foreground"
                  />
                  <span className="truncate text-sm">{proxy.name}</span>
                </div>
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {proxy.proxy_type.toUpperCase()} :{proxy.local_port}
                </span>
              </div>
            ))
          ) : (
            <p className="py-5 text-center text-sm text-muted-foreground">
              {t("binding.noAssignedProxies")}
            </p>
          )}
        </div>
      </CardContent>
      <CardFooter className="flex flex-wrap justify-between gap-2 border-t bg-muted/20 py-3">
        <Button
          variant="outline"
          size="sm"
          aria-label={`${t("binding.configure")} ${profile.name}`}
          onClick={onConfigure}
        >
          <AppIcon name="status" />
          {t("binding.configure")}
        </Button>
        <div className="flex gap-2">
          {(state?.running || state?.desired_running) && (
            <ActionButton
              variant="outline"
              size="sm"
              busy={action.isPending}
              onClick={() => action.mutate("stop")}
            >
              <AppIcon name="stop" />
              {t("binding.stopProfile")}
            </ActionButton>
          )}
          <ActionButton
            size="sm"
            busy={action.isPending}
            disabled={runtime.isPending || !!runtime.error || !proxies.length}
            onClick={() => action.mutate("start")}
          >
            <AppIcon name={state?.running ? "refresh" : "play"} />
            {state?.running
              ? state.config_pending
                ? t("binding.applyUpdate")
                : t("app.reload")
              : t("binding.startProfile")}
          </ActionButton>
        </div>
      </CardFooter>
    </Card>
  );
}
