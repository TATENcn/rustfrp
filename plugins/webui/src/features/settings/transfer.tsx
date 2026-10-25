import { useRef, useState } from "react";
import { toast } from "sonner";
import { downloadBackup, importFrpcToml } from "@/api/config";
import { data, useAction } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { AppIcon } from "@/components/icon/AppIcon";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { ActionButton } from "@/components/common/page";
export function Transfer() {
  const { t, text } = useI18n(),
    input = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("imported"),
    [file, setFile] = useState<File | null>(null);
  const importing = useAction(async () => {
    if (!file || !name.trim()) throw new Error(t("configTransfer.selectFile"));
    const result = await data(importFrpcToml(name.trim(), await file.text()));
    toast.success(
      t("configTransfer.importSuccess", {
        profile: result.profile_name,
        proxies: result.proxies_imported,
        visitors: result.visitors_imported,
      }),
    );
    setFile(null);
    if (input.current) input.current.value = "";
  });
  const exporting = useAction(
    () => downloadBackup(),
    t("configTransfer.exportSuccess"),
  );
  return (
    <div className="grid items-start gap-6 lg:grid-cols-2">
      <Card className="shadow-none">
        <CardHeader>
          <AppIcon
            name="upload"
            className="mb-2 text-muted-foreground"
            size={22}
          />
          <CardTitle>{t("configTransfer.importTitle")}</CardTitle>
          <CardDescription>{t("configTransfer.importHint")}</CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="space-y-5"
            onSubmit={(e) => {
              e.preventDefault();
              importing.mutate();
            }}
          >
            <Field>
              <FieldLabel htmlFor="profile-name">
                {t("configTransfer.profileName")}
              </FieldLabel>
              <Input
                id="profile-name"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={importing.isPending}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="config-file">
                {t("configTransfer.chooseFile")}
              </FieldLabel>
              <Input
                id="config-file"
                ref={input}
                type="file"
                accept=".toml,text/plain"
                required
                onChange={(e) => setFile(e.target.files?.[0] ?? null)}
                disabled={importing.isPending}
              />
              <FieldDescription>
                {text(
                  "导入为新的服务器配置及关联代理。",
                  "Creates a server profile and its associated proxies.",
                )}
              </FieldDescription>
            </Field>
            <ActionButton
              type="submit"
              busy={importing.isPending}
              disabled={!file || !name.trim()}
            >
              {t("configTransfer.import")}
            </ActionButton>
          </form>
        </CardContent>
      </Card>
      <Card className="shadow-none">
        <CardHeader>
          <AppIcon
            name="download"
            className="mb-2 text-muted-foreground"
            size={22}
          />
          <CardTitle>{t("configTransfer.exportTitle")}</CardTitle>
          <CardDescription>{t("configTransfer.exportHint")}</CardDescription>
        </CardHeader>
        <CardContent>
          <ActionButton
            variant="outline"
            busy={exporting.isPending}
            onClick={() => exporting.mutate()}
          >
            {t("configTransfer.export")}
          </ActionButton>
        </CardContent>
      </Card>
    </div>
  );
}
