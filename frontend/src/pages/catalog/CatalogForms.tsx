import { Button, Grid, Group, Modal, Select, Stack, Text, Textarea, TextInput } from "@mantine/core";
import { useState } from "react";
import type { FormEvent } from "react";

import type {
  ApiId,
  Maintainer,
  MaintainerMember,
  MaintainerMemberPayload,
  NewMaintainerPayload,
  NewPackagePayload,
  NewServicePayload,
  Package,
  Service
} from "../../types/api";

const lifecycleOptions = ["active", "deprecated", "archived"];
const healthOptions = ["healthy", "degraded", "down", "unknown"];
const memberRoleOptions = ["owner", "maintainer", "viewer"];

export type CatalogDialog =
  | { type: "maintainer"; record?: Maintainer }
  | { type: "service"; record?: Service }
  | { type: "package"; record?: Package }
  | { type: "member"; maintainerId: ApiId; record?: MaintainerMember }
  | null;

export type SelectOption = {
  value: string;
  label: string;
};

type ServiceFormState = Omit<
  NewServicePayload,
  "maintainer_id" | "external_id" | "description" | "repository_url" | "dashboard_url" | "runbook_url" | "last_checked_at"
> & {
  maintainer_id: string;
  external_id: string;
  description: string;
  repository_url: string;
  dashboard_url: string;
  runbook_url: string;
};

type PackageFormState = Omit<
  NewPackagePayload,
  "maintainer_id" | "description" | "repository_url" | "documentation_url"
> & {
  maintainer_id: string;
  description: string;
  repository_url: string;
  documentation_url: string;
};

type MemberFormState = {
  user_id: string;
  role: string;
};

export function CatalogManagementModal({
  dialog,
  title,
  onClose,
  maintainerOptions,
  defaultMaintainerId,
  userOptions,
  saving,
  onSaveMaintainer,
  onSaveService,
  onSavePackage,
  onSaveMember
}: {
  dialog: CatalogDialog;
  title: string;
  onClose: () => void;
  maintainerOptions: SelectOption[];
  defaultMaintainerId: string;
  userOptions: SelectOption[];
  saving: boolean;
  onSaveMaintainer: (payload: NewMaintainerPayload) => void | Promise<void>;
  onSaveService: (payload: NewServicePayload) => void | Promise<void>;
  onSavePackage: (payload: NewPackagePayload) => void | Promise<void>;
  onSaveMember: (payload: MaintainerMemberPayload) => void | Promise<void>;
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
      {dialog?.type === "member" && (
        <MaintainerMemberForm
          key={formKey}
          initialValue={dialog.record}
          userOptions={userOptions}
          submitting={saving}
          onCancel={onClose}
          onSubmit={onSaveMember}
        />
      )}
    </Modal>
  );
}

export function RemoveMemberModal({
  member,
  userLabel,
  maintainerLabel,
  removing,
  onCancel,
  onConfirm
}: {
  member: MaintainerMember | null;
  userLabel: string;
  maintainerLabel: string;
  removing: boolean;
  onCancel: () => void;
  onConfirm: (member: MaintainerMember) => void | Promise<void>;
}) {
  return (
    <Modal opened={Boolean(member)} onClose={onCancel} title="Remove member" size="sm" centered>
      <Stack>
        <Text size="sm">
          Remove {userLabel || "this user"} from {maintainerLabel || "this maintainer"}?
        </Text>
        <Group justify="flex-end">
          <Button type="button" variant="default" onClick={onCancel} disabled={removing}>
            Cancel
          </Button>
          <Button
            color="red"
            loading={removing}
            onClick={() => {
              if (member) {
                onConfirm(member);
              }
            }}
          >
            Remove
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}

function MaintainerMemberForm({
  initialValue,
  userOptions,
  submitting,
  onCancel,
  onSubmit
}: {
  initialValue?: MaintainerMember;
  userOptions: SelectOption[];
  submitting: boolean;
  onCancel: () => void;
  onSubmit: (payload: MaintainerMemberPayload) => void | Promise<void>;
}) {
  const [form, setForm] = useState<MemberFormState>({
    user_id: initialValue?.user_id ? String(initialValue.user_id) : userOptions[0]?.value || "",
    role: initialValue?.role || "maintainer"
  });

  function update(field: keyof MemberFormState, value: string) {
    setForm((current) => ({ ...current, [field]: value }));
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({
      user_id: Number(form.user_id),
      role: form.role
    });
  }

  return (
    <form onSubmit={submit}>
      <Stack>
        <Select
          label="User"
          data={userOptions}
          value={form.user_id}
          onChange={(value) => update("user_id", value || "")}
          disabled={Boolean(initialValue)}
          searchable
          required
        />
        <Select
          label="Role"
          data={memberRoleOptions}
          value={form.role}
          onChange={(value) => update("role", value || "maintainer")}
          required
        />
        <FormActions
          submitting={submitting}
          onCancel={onCancel}
          submitLabel={initialValue ? "Save role" : "Add member"}
        />
      </Stack>
    </form>
  );
}

function MaintainerForm({
  initialValue,
  submitting,
  onCancel,
  onSubmit
}: {
  initialValue?: Maintainer;
  submitting: boolean;
  onCancel: () => void;
  onSubmit: (payload: NewMaintainerPayload) => void | Promise<void>;
}) {
  const [form, setForm] = useState<NewMaintainerPayload>({
    display_name: initialValue?.display_name || "",
    email: initialValue?.email || ""
  });

  function update(field: keyof NewMaintainerPayload, value: string) {
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
}: {
  initialValue?: Service;
  maintainerOptions: SelectOption[];
  defaultMaintainerId: string;
  submitting: boolean;
  onCancel: () => void;
  onSubmit: (payload: NewServicePayload) => void | Promise<void>;
}) {
  const [form, setForm] = useState<ServiceFormState>({
    maintainer_id: initialValue?.maintainer_id
      ? String(initialValue.maintainer_id)
      : defaultMaintainerId,
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

  function update(field: keyof ServiceFormState, value: string) {
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
}: {
  initialValue?: Package;
  maintainerOptions: SelectOption[];
  defaultMaintainerId: string;
  submitting: boolean;
  onCancel: () => void;
  onSubmit: (payload: NewPackagePayload) => void | Promise<void>;
}) {
  const [form, setForm] = useState<PackageFormState>({
    maintainer_id: initialValue?.maintainer_id
      ? String(initialValue.maintainer_id)
      : defaultMaintainerId,
    slug: initialValue?.slug || "",
    name: initialValue?.name || "",
    version: initialValue?.version || "",
    status: initialValue?.status || "active",
    description: initialValue?.description || "",
    repository_url: initialValue?.repository_url || "",
    documentation_url: initialValue?.documentation_url || ""
  });

  function update(field: keyof PackageFormState, value: string) {
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

function FormActions({
  submitting,
  onCancel,
  submitLabel
}: {
  submitting: boolean;
  onCancel: () => void;
  submitLabel: string;
}) {
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

export function getDialogTitle(dialog: CatalogDialog): string {
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
  if (dialog.type === "package") {
    return `${action} package`;
  }
  return dialog.record ? "Edit member role" : "Add member";
}

function optionalText(value: unknown): string | null {
  const trimmed = String(value || "").trim();
  return trimmed.length > 0 ? trimmed : null;
}
