import { Suspense } from "react";
import {
  Link,
  NavLink,
  Outlet,
  useLocation,
  useNavigate,
} from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { useI18n } from "@/i18n";
import { useEnvironments, useStatus } from "@/hooks/api";
import { usePreferences, type ThemeMode } from "@/hooks/preferences";
import { AppIcon, type IconName } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { Choice, PageLoading, RuntimeBadge } from "@/components/common/page";
type NavKey =
  | "dashboard"
  | "profiles"
  | "proxies"
  | "bindings"
  | "visitors"
  | "logs"
  | "status";
const sections: { key: string; items: [NavKey, string][] }[] = [
  { key: "overview", items: [["dashboard", "/"]] },
  {
    key: "configuration",
    items: [
      ["profiles", "/profiles"],
      ["proxies", "/proxies"],
      ["bindings", "/bindings"],
      ["visitors", "/visitors"],
    ],
  },
  {
    key: "management",
    items: [
      ["logs", "/logs"],
      ["status", "/status"],
    ],
  },
];
function Navigation() {
  const { t, text } = useI18n();
  const location = useLocation();
  const { setOpenMobile } = useSidebar();
  const prefs = usePreferences(),
    environments = useEnvironments();
  const active =
    environments.data?.find((e) => e.id === prefs.environmentId) ??
    environments.data?.find((e) => e.is_default);
  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader className="p-3">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to="/">
                <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                  <AppIcon name="proxies" size={18} />
                </div>
                <div className="grid leading-tight">
                  <span className="font-semibold">RustFRP</span>
                  <span className="text-xs text-muted-foreground">
                    {text("网络控制台", "Network console")}
                  </span>
                </div>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <div className="px-4 pt-1 group-data-[collapsible=icon]:hidden">
          <Choice
            label={t("app.environment")}
            value={active?.id}
            options={(environments.data ?? []).map((e) => ({
              label: e.name,
              value: e.id!,
            }))}
            onChange={(v) => prefs.selectEnvironment(Number(v))}
          />
        </div>
        {sections.map((section) => (
          <SidebarGroup key={section.key}>
            <SidebarGroupLabel>
              {section.key === "overview"
                ? text("工作空间", "Workspace")
                : section.key === "configuration"
                  ? text("配置管理", "Configuration")
                  : text("运维管理", "Management")}
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {section.items.map(([key, path]) => (
                  <SidebarMenuItem key={key}>
                    <SidebarMenuButton
                      asChild
                      isActive={
                        path === "/"
                          ? location.pathname === "/"
                          : location.pathname.startsWith(path)
                      }
                      tooltip={t(`nav.${key}`)}
                    >
                      <NavLink
                        to={path}
                        end={path === "/"}
                        onClick={() => setOpenMobile(false)}
                      >
                        <AppIcon name={key as IconName} />
                        <span>{t(`nav.${key}`)}</span>
                      </NavLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ))}
      </SidebarContent>
      <SidebarFooter className="p-4">
        <div className="flex items-center gap-2 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
          <AppIcon name="shield" />
          <span>
            RustFRP <span className="ml-1">v0.3.0</span>
          </span>
        </div>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
function Header() {
  const { t, text, locale, setLocale } = useI18n(),
    prefs = usePreferences(),
    status = useStatus();
  const location = useLocation(),
    navigate = useNavigate(),
    client = useQueryClient();
  const { isMobile } = useSidebar();
  const key =
    sections
      .flatMap((s) => s.items)
      .find(
        ([, path]) => path !== "/" && location.pathname.startsWith(path),
      )?.[0] ?? "dashboard";
  return (
    <header className="flex h-16 shrink-0 items-center justify-between gap-3 border-b px-4 md:px-6">
      <div className="flex min-w-0 items-center gap-3">
        <SidebarTrigger
          aria-label={
            isMobile
              ? text("打开导航", "Open navigation")
              : text("切换导航", "Toggle navigation")
          }
        />
        <Separator orientation="vertical" className="!h-4" />
        <Breadcrumb>
          <BreadcrumbList>
            <BreadcrumbItem className="hidden sm:block">RustFRP</BreadcrumbItem>
            <BreadcrumbSeparator className="hidden sm:block" />
            <BreadcrumbItem>
              <BreadcrumbPage>{t(`nav.${key}`)}</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
      </div>
      <div className="flex items-center gap-1">
        <span className="mr-3 hidden lg:block">
          <RuntimeBadge running={!!status.data && !status.error}>
            {status.error
              ? text("连接异常", "Connection error")
              : status.isPending
                ? text("正在连接", "Connecting")
                : text("服务在线", "System online")}
          </RuntimeBadge>
        </span>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("app.language")}
          onClick={() => setLocale(locale === "zh" ? "en" : "zh")}
        >
          <AppIcon name="language" />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              aria-label={t("app.appearance")}
            >
              <AppIcon name="palette" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{t("app.appearance")}</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={prefs.mode}
              onValueChange={(v) => prefs.setMode(v as ThemeMode)}
            >
              {(["light", "dark", "system"] as const).map((mode, i) => (
                <DropdownMenuRadioItem key={mode} value={mode}>
                  {text(
                    ["浅色", "深色", "跟随系统"][i]!,
                    ["Light", "Dark", "System"][i]!,
                  )}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("auth.logout")}
          onClick={() => {
            localStorage.removeItem("api_token");
            client.clear();
            navigate("/login", { replace: true });
          }}
        >
          <AppIcon name="logout" />
        </Button>
      </div>
    </header>
  );
}
export default function Shell() {
  return (
    <SidebarProvider>
      <Navigation />
      <SidebarInset className="min-w-0">
        <Header />
        <div className="mx-auto w-full max-w-[1440px] flex-1 p-4 md:p-6 lg:p-8">
          <Suspense fallback={<PageLoading />}>
            <Outlet />
          </Suspense>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
