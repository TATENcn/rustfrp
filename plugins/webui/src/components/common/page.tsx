import type { ComponentProps, ReactNode } from "react";
import { useI18n } from "@/i18n";
import { extractApiError } from "@/api/errors";
import { AppIcon, type IconName } from "@/components/icon/AppIcon";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  EmptyContent,
} from "@/components/ui/empty";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
export function PageHeading({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-start">
      <div className="space-y-1.5">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        <p className="max-w-2xl text-sm leading-6 text-muted-foreground">
          {description}
        </p>
      </div>
      {children && (
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {children}
        </div>
      )}
    </div>
  );
}
export function ErrorState({
  error,
  retry,
}: {
  error: unknown;
  retry?: () => void;
}) {
  const { text, t } = useI18n();
  if (!error) return null;
  return (
    <Alert variant="destructive">
      <AppIcon name="alert" />
      <AlertTitle>{text("无法加载数据", "Unable to load data")}</AlertTitle>
      <AlertDescription>
        <p className="break-words">{extractApiError(error).message}</p>
        {retry && (
          <Button
            variant="outline"
            size="sm"
            className="mt-2 text-foreground"
            onClick={retry}
          >
            {t("common.refresh")}
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}
export function EmptyState({
  title,
  description,
  icon = "proxies",
  children,
}: {
  title?: string;
  description?: string;
  icon?: IconName;
  children?: ReactNode;
}) {
  const { text } = useI18n();
  return (
    <Empty className="min-h-64">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <AppIcon name={icon} />
        </EmptyMedia>
        <EmptyTitle>{title ?? text("暂无数据", "No data yet")}</EmptyTitle>
        <EmptyDescription>
          {description ??
            text(
              "添加配置后，相关数据会显示在这里。",
              "Your data will appear here when it is available.",
            )}
        </EmptyDescription>
      </EmptyHeader>
      {children && <EmptyContent>{children}</EmptyContent>}
    </Empty>
  );
}
export function PageLoading() {
  return (
    <div role="status" aria-label="Loading" className="space-y-6">
      <Skeleton className="h-12 w-60" />
      <div className="grid gap-4 sm:grid-cols-3">
        {[1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-28" />
        ))}
      </div>
      <Skeleton className="h-80" />
    </div>
  );
}
export function ActionButton({
  busy,
  children,
  disabled,
  ...props
}: ComponentProps<typeof Button> & { busy?: boolean }) {
  return (
    <Button disabled={busy || disabled} {...props}>
      {busy && <AppIcon name="loading" className="animate-spin" />}
      {children}
    </Button>
  );
}
export function Choice({
  label,
  value,
  options,
  onChange,
  className,
  disabled,
  id,
}: {
  label: string;
  value?: number | string | null;
  options: { label: string; value: string | number }[];
  onChange: (value: string) => void;
  className?: string;
  disabled?: boolean;
  id?: string;
}) {
  return (
    <Select
      value={value == null ? "" : String(value)}
      onValueChange={onChange}
      disabled={disabled}
    >
      <SelectTrigger
        id={id}
        aria-label={label}
        className={className ?? "w-full"}
      >
        <SelectValue placeholder={label} />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={String(option.value)}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
export function RuntimeBadge({
  running,
  children,
}: {
  running?: boolean;
  children?: ReactNode;
}) {
  const { text } = useI18n();
  return (
    <Badge variant="outline" className="gap-1.5 font-normal">
      <span
        className={`size-1.5 rounded-full ${running ? "bg-emerald-500" : "bg-muted-foreground"}`}
      />
      {children ??
        (running ? text("运行中", "Running") : text("已停止", "Stopped"))}
    </Badge>
  );
}
