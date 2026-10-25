import { useState } from "react";
import { toast } from "sonner";
import { apiPost } from "@/api/client";
import type { LocalProxy } from "@/api/types";
import { data, useAction } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { defaultProxy, parsePortSpec } from "@/lib/entities";
import { EditorDialog } from "@/components/common/dialogs";
import { ActionButton, Choice } from "@/components/common/page";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
export function BatchDialog({
  existing,
  onClose,
}: {
  existing: LocalProxy[];
  onClose: () => void;
}) {
  const { t, text } = useI18n();
  const [spec, setSpec] = useState(""),
    [type, setType] = useState("tcp"),
    [ip, setIp] = useState("127.0.0.1");
  const create = useAction(
    async () => {
      let ports: number[];
      try {
        ports = parsePortSpec(spec);
      } catch (error) {
        throw new Error(
          t(
            error instanceof Error && error.message === "too_many"
              ? "proxy.batchTooMany"
              : "proxy.batchInvalid",
          ),
        );
      }
      if (!ports.length || !ip.trim()) throw new Error(t("proxy.batchInvalid"));
      const conflicts = ports.filter((port) =>
        existing.some((p) => p.name === `${type}-${port}`),
      );
      if (conflicts.length)
        throw new Error(
          t("proxy.batchConflict", { ports: conflicts.join(", ") }),
        );
      let created = 0;
      try {
        for (const port of ports) {
          await data(
            apiPost("/proxies", {
              ...defaultProxy(),
              name: `${type}-${port}`,
              proxy_type: type,
              local_ip: ip.trim(),
              local_port: port,
              remote_port: port,
            }),
          );
          created++;
        }
      } catch {
        throw new Error(t("proxy.batchPartial", { created }));
      }
      toast.success(t("proxy.batchSuccess", { count: created }));
    },
    undefined,
    onClose,
  );
  return (
    <EditorDialog
      open
      title={t("proxy.batchCreate")}
      description={text(
        "一次添加多个 TCP 或 UDP 端口映射。",
        "Create multiple TCP or UDP port mappings at once.",
      )}
      busy={create.isPending}
      onClose={onClose}
    >
      <form
        className="space-y-5"
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate();
        }}
      >
        <fieldset disabled={create.isPending} className="space-y-5">
          <Field>
            <FieldLabel htmlFor="ports">{t("proxy.batchPorts")}</FieldLabel>
            <Input
              id="ports"
              required
              value={spec}
              onChange={(e) => setSpec(e.target.value)}
              placeholder="8080, 9000-9005"
            />
            <FieldDescription>
              {text(
                "支持单个端口、逗号列表或连续范围，最多 100 个。",
                "Single ports, comma-separated lists, or ranges. Up to 100 ports.",
              )}
            </FieldDescription>
          </Field>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field>
              <FieldLabel htmlFor="batch-type">{t("proxy.type")}</FieldLabel>
              <Choice
                id="batch-type"
                label={t("proxy.type")}
                value={type}
                onChange={setType}
                options={["tcp", "udp"].map((v) => ({
                  value: v,
                  label: v.toUpperCase(),
                }))}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="batch-ip">{t("proxy.localIp")}</FieldLabel>
              <Input
                id="batch-ip"
                required
                value={ip}
                onChange={(e) => setIp(e.target.value)}
              />
            </Field>
          </div>
        </fieldset>
        <div className="flex justify-end gap-2 border-t pt-4">
          <Button
            type="button"
            variant="outline"
            disabled={create.isPending}
            onClick={onClose}
          >
            {t("common.cancel")}
          </Button>
          <ActionButton type="submit" busy={create.isPending}>
            {t("common.save")}
          </ActionButton>
        </div>
      </form>
    </EditorDialog>
  );
}
