import { Button, Grid, Group, Modal, Select, Stack, Text, Textarea, TextInput } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { IconPencil, IconPlus } from "@tabler/icons-react";
import { useMemo, useState } from "react";
import type { FormEvent } from "react";

import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { CatalogSkeleton } from "../../components/LoadingState";
import { LinkCell, StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import { showError } from "../../utils/notifications";

const lifecycleOptions = ["active", "deprecated", "archived"];
const healthOptions = ["healthy", "degraded", "down", "unknown"];

type CatalogDialog =
  | { type: "maintainer"; record?: any }
  | { type: "service"; record?: any }
  | { type: "package"; record?: any }
  | null;

export function CatalogView({ client }) {
  const [dialog, setDialog] = useState<CatalogDialog>(null);
  const [saving, setSaving] = useState(false);
  const [data, actions] = useAsyncData(async () => {
    const [maintainers, services, packages] = await Promise.all([
      client.get("/maintainers"),
      client.get("/services"),
      client.get("/packages")
    ]);
    return { maintainers, services, packages };
  }, [client]);

  useRefresh(actions.reload);

  const catalog = data.value;
  const maintainerOptions = useMemo(
    () =>
      (catalog?.maintainers || []).map((maintainer) => ({
        value: String(maintainer.id),
        label: `${maintainer.display_name} <${maintainer.email}>`
      })),
    [catalog?.maintainers]
  );
  const maintainerById = useMemo(() => {
    const lookup = new Map();
    (catalog?.maintainers || []).forEach((maintainer) => lookup.set(maintainer.id, maintainer));
    return lookup;
  }, [catalog?.maintainers]);
  const defaultMaintainerId = maintainerOptions[0]?.value || "";
  const canManageOwnedRecords = maintainerOptions.length > 0;
  const dialogTitle = getDialogTitle(dialog);

  async function saveMaintainer(payload) {
    if (!dialog || dialog.type !== "maintainer") {
      return;
    }

    setSaving(true);
    try {
      const maintainer = dialog.record
        ? await client.put(`/maintainers/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post("/maintainers", payload);
      await actions.reload();
      setDialog(null);
      notifications.show({
        title: dialog.record ? "Maintainer updated" : "Maintainer created",
        message: maintainer.display_name,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setSaving(false);
    }
  }

  async function saveService(payload) {
    if (!dialog || dialog.type !== "service") {
      return;
    }

    setSaving(true);
    try {
      const service = dialog.record
        ? await client.put(`/services/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post("/services", payload);
      await actions.reload();
      setDialog(null);
      notifications.show({
        title: dialog.record ? "Service updated" : "Service created",
        message: service.name,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setSaving(false);
    }
  }

  async function savePackage(payload) {
    if (!dialog || dialog.type !== "package") {
      return;
    }

    setSaving(true);
    try {
      const packageRecord = dialog.record
        ? await client.put(`/packages/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post("/packages", payload);
      await actions.reload();
      setDialog(null);
      notifications.show({
        title: dialog.record ? "Package updated" : "Package created",
        message: packageRecord.name,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setSaving(false);
    }
  }

  const MaintainerActionCell = ({ row }) => (
    <EditAction
      label={`Edit maintainer ${row.display_name}`}
      onClick={() => setDialog({ type: "maintainer", record: row })}
    />
  );
  const ServiceActionCell = ({ row }) => (
    <EditAction
      label={`Edit service ${row.name}`}
      onClick={() => setDialog({ type: "service", record: row })}
    />
  );
  const PackageActionCell = ({ row }) => (
    <EditAction
      label={`Edit package ${row.name}`}
      onClick={() => setDialog({ type: "package", record: row })}
    />
  );
  const MaintainerCell = ({ value }) => {
    const maintainer = maintainerById.get(value);
    return <Text size="sm">{maintainer?.display_name || "-"}</Text>;
  };

  return (
    <ViewFrame
      eyebrow="Ownership"
      title="Catalog"
      loading={data.loading && !data.value}
      loadingFallback={<CatalogSkeleton />}
      error={data.error}
    >
      {catalog && (
        <>
          <CatalogManagementModal
            dialog={dialog}
            title={dialogTitle}
            onClose={() => setDialog(null)}
            maintainerOptions={maintainerOptions}
            defaultMaintainerId={defaultMaintainerId}
            saving={saving}
            onSaveMaintainer={saveMaintainer}
            onSaveService={saveService}
            onSavePackage={savePackage}
          />

          <Stack gap="lg">
            <DataPanel
              title="Maintainers"
              actions={
                <Button
                  leftSection={<IconPlus size={16} />}
                  size="compact-sm"
                  onClick={() => setDialog({ type: "maintainer" })}
                >
                  New maintainer
                </Button>
              }
            >
              <DataTable
                rows={catalog.maintainers}
                columns={[
                  ["display_name", "Maintainer"],
                  ["email", "Email"],
                  ["_actions", "Actions", MaintainerActionCell]
                ]}
              />
            </DataPanel>

            <Grid>
              <Grid.Col span={{ base: 12, xl: 7 }}>
                <DataPanel
                  title="Services"
                  actions={
                    <Button
                      leftSection={<IconPlus size={16} />}
                      size="compact-sm"
                      disabled={!canManageOwnedRecords}
                      onClick={() => setDialog({ type: "service" })}
                    >
                      New service
                    </Button>
                  }
                >
                  <DataTable
                    rows={catalog.services}
                    columns={[
                      ["name", "Service"],
                      ["maintainer_id", "Maintainer", MaintainerCell],
                      ["health_status", "Health", StatusBadge],
                      ["lifecycle_status", "Lifecycle", StatusBadge],
                      ["source", "Source", SourceCell],
                      ["_actions", "Actions", ServiceActionCell]
                    ]}
                  />
                </DataPanel>
              </Grid.Col>
              <Grid.Col span={{ base: 12, xl: 5 }}>
                <DataPanel
                  title="Packages"
                  actions={
                    <Button
                      leftSection={<IconPlus size={16} />}
                      size="compact-sm"
                      disabled={!canManageOwnedRecords}
                      onClick={() => setDialog({ type: "package" })}
                    >
                      New package
                    </Button>
                  }
                >
                  <DataTable
                    rows={catalog.packages}
                    columns={[
                      ["name", "Package"],
                      ["maintainer_id", "Maintainer", MaintainerCell],
                      ["version", "Version"],
                      ["status", "Status", StatusBadge],
                      ["repository_url", "Repo", LinkCell],
                      ["_actions", "Actions", PackageActionCell]
                    ]}
                  />
                </DataPanel>
              </Grid.Col>
            </Grid>
          </Stack>
        </>
      )}
    </ViewFrame>
  );
}

function CatalogManagementModal({
  dialog,
  title,
  onClose,
  maintainerOptions,
  defaultMaintainerId,
  saving,
  onSaveMaintainer,
  onSaveService,
  onSavePackage
}) {
  const opened = Boolean(dialog);
  const formKey = dialog ? `${dialog.type}-${dialog.record?.id || "new"}` : "closed";

  return (
    <Modal opened={opened} onClose={onClose} title={title} size="lg" centered>
      {dialog?.type === "maintainer" && (
        <MaintainerForm
          key={formKey}
          initialValue={dialog.record}
          submitting={saving}
          onCancel={onClose}
          onSubmit={onSaveMaintainer}
        />
      )}
      {dialog?.type === "service" && (
        <ServiceForm
          key={formKey}
          initialValue={dialog.record}
          maintainerOptions={maintainerOptions}
          defaultMaintainerId={defaultMaintainerId}
          submitting={saving}
          onCancel={onClose}
          onSubmit={onSaveService}
        />
      )}
      {dialog?.type === "package" && (
        <PackageForm
          key={formKey}
          initialValue={dialog.record}
          maintainerOptions={maintainerOptions}
          defaultMaintainerId={defaultMaintainerId}
          submitting={saving}
          onCancel={onClose}
          onSubmit={onSavePackage}
        />
      )}
    </Modal>
  );
}

function MaintainerForm({ initialValue, submitting, onCancel, onSubmit }) {
  const [form, setForm] = useState({
    display_name: initialValue?.display_name || "",
    email: initialValue?.email || ""
  });

  function update(field, value) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({
      display_name: form.display_name.trim(),
      email: form.email.trim()
    });
  }

  return (
    <form onSubmit={submit}>
      <Stack>
        <TextInput
          label="Display name"
          value={form.display_name}
          onChange={(event) => update("display_name", event.currentTarget.value)}
          required
        />
        <TextInput
          label="Email"
          value={form.email}
          onChange={(event) => update("email", event.currentTarget.value)}
          required
        />
        <FormActions
          submitting={submitting}
          onCancel={onCancel}
          submitLabel={initialValue ? "Save maintainer" : "Create maintainer"}
        />
      </Stack>
    </form>
  );
}

function ServiceForm({
  initialValue,
  maintainerOptions,
  defaultMaintainerId,
  submitting,
  onCancel,
  onSubmit
}) {
  const [form, setForm] = useState({
    maintainer_id: initialValue?.maintainer_id ? String(initialValue.maintainer_id) : defaultMaintainerId,
    slug: initialValue?.slug || "",
    name: initialValue?.name || "",
    lifecycle_status: initialValue?.lifecycle_status || "active",
    health_status: initialValue?.health_status || "unknown",
    source: initialValue?.source || "manual",
    external_id: initialValue?.external_id || "",
    description: initialValue?.description || "",
    repository_url: initialValue?.repository_url || "",
    dashboard_url: initialValue?.dashboard_url || "",
    runbook_url: initialValue?.runbook_url || ""
  });

  function update(field, value) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({
      source: form.source.trim() || "manual",
      external_id: optionalText(form.external_id),
      maintainer_id: Number(form.maintainer_id),
      slug: form.slug.trim(),
      name: form.name.trim(),
      lifecycle_status: form.lifecycle_status,
      health_status: form.health_status,
      description: optionalText(form.description),
      repository_url: optionalText(form.repository_url),
      dashboard_url: optionalText(form.dashboard_url),
      runbook_url: optionalText(form.runbook_url),
      last_checked_at: initialValue?.last_checked_at || null
    });
  }

  return (
    <form onSubmit={submit}>
      <Stack>
        <Grid>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <Select
              label="Maintainer"
              data={maintainerOptions}
              value={form.maintainer_id}
              onChange={(value) => update("maintainer_id", value || "")}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Source"
              value={form.source}
              onChange={(event) => update("source", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Slug"
              value={form.slug}
              onChange={(event) => update("slug", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Name"
              value={form.name}
              onChange={(event) => update("name", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <Select
              label="Lifecycle"
              data={lifecycleOptions}
              value={form.lifecycle_status}
              onChange={(value) => update("lifecycle_status", value || "active")}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <Select
              label="Health"
              data={healthOptions}
              value={form.health_status}
              onChange={(value) => update("health_status", value || "unknown")}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="External ID"
              value={form.external_id}
              onChange={(event) => update("external_id", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Repository URL"
              value={form.repository_url}
              onChange={(event) => update("repository_url", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Dashboard URL"
              value={form.dashboard_url}
              onChange={(event) => update("dashboard_url", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Runbook URL"
              value={form.runbook_url}
              onChange={(event) => update("runbook_url", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={12}>
            <Textarea
              label="Description"
              minRows={3}
              autosize
              value={form.description}
              onChange={(event) => update("description", event.currentTarget.value)}
            />
          </Grid.Col>
        </Grid>
        <FormActions
          submitting={submitting}
          onCancel={onCancel}
          submitLabel={initialValue ? "Save service" : "Create service"}
        />
      </Stack>
    </form>
  );
}

function PackageForm({
  initialValue,
  maintainerOptions,
  defaultMaintainerId,
  submitting,
  onCancel,
  onSubmit
}) {
  const [form, setForm] = useState({
    maintainer_id: initialValue?.maintainer_id ? String(initialValue.maintainer_id) : defaultMaintainerId,
    slug: initialValue?.slug || "",
    name: initialValue?.name || "",
    version: initialValue?.version || "",
    status: initialValue?.status || "active",
    description: initialValue?.description || "",
    repository_url: initialValue?.repository_url || "",
    documentation_url: initialValue?.documentation_url || ""
  });

  function update(field, value) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({
      maintainer_id: Number(form.maintainer_id),
      slug: form.slug.trim(),
      name: form.name.trim(),
      version: form.version.trim(),
      status: form.status,
      description: optionalText(form.description),
      repository_url: optionalText(form.repository_url),
      documentation_url: optionalText(form.documentation_url)
    });
  }

  return (
    <form onSubmit={submit}>
      <Stack>
        <Grid>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <Select
              label="Maintainer"
              data={maintainerOptions}
              value={form.maintainer_id}
              onChange={(value) => update("maintainer_id", value || "")}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <Select
              label="Status"
              data={lifecycleOptions}
              value={form.status}
              onChange={(value) => update("status", value || "active")}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Slug"
              value={form.slug}
              onChange={(event) => update("slug", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Name"
              value={form.name}
              onChange={(event) => update("name", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Version"
              value={form.version}
              onChange={(event) => update("version", event.currentTarget.value)}
              required
            />
          </Grid.Col>
          <Grid.Col span={{ base: 12, md: 6 }}>
            <TextInput
              label="Repository URL"
              value={form.repository_url}
              onChange={(event) => update("repository_url", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={12}>
            <TextInput
              label="Documentation URL"
              value={form.documentation_url}
              onChange={(event) => update("documentation_url", event.currentTarget.value)}
            />
          </Grid.Col>
          <Grid.Col span={12}>
            <Textarea
              label="Description"
              minRows={3}
              autosize
              value={form.description}
              onChange={(event) => update("description", event.currentTarget.value)}
            />
          </Grid.Col>
        </Grid>
        <FormActions
          submitting={submitting}
          onCancel={onCancel}
          submitLabel={initialValue ? "Save package" : "Create package"}
        />
      </Stack>
    </form>
  );
}

function FormActions({ submitting, onCancel, submitLabel }) {
  return (
    <Group justify="flex-end" mt="sm">
      <Button type="button" variant="default" onClick={onCancel}>
        Cancel
      </Button>
      <Button type="submit" loading={submitting}>
        {submitLabel}
      </Button>
    </Group>
  );
}

function EditAction({ label, onClick }) {
  return (
    <Button
      variant="subtle"
      size="compact-sm"
      leftSection={<IconPencil size={16} />}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      Edit
    </Button>
  );
}

function SourceCell({ value }) {
  if (!value) {
    return null;
  }

  return (
    <Text size="sm" className="catalogSourceCell" title={String(value)}>
      {String(value)}
    </Text>
  );
}

function getDialogTitle(dialog: CatalogDialog) {
  if (!dialog) {
    return "";
  }

  const action = dialog.record ? "Edit" : "Create";
  if (dialog.type === "maintainer") {
    return `${action} maintainer`;
  }
  if (dialog.type === "service") {
    return `${action} service`;
  }
  return `${action} package`;
}

function optionalText(value) {
  const trimmed = String(value || "").trim();
  return trimmed.length > 0 ? trimmed : null;
}
