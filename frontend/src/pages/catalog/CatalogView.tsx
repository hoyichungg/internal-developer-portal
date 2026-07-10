import { Button, Grid, Group, Select, Stack, Text } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { IconPencil, IconPlus, IconTrash, IconUserPlus, IconUsers } from "@tabler/icons-react";
import { useEffect, useMemo, useState } from "react";

import type { ApiClient } from "../../api/client";
import { DataPanel } from "../../components/DataPanel";
import { DataTable } from "../../components/DataTable";
import { CatalogSkeleton } from "../../components/LoadingState";
import { DateCell, LinkCell, StatusBadge } from "../../components/tableCells";
import { ViewFrame } from "../../components/ViewFrame";
import { useAsyncData } from "../../hooks/useAsyncData";
import { useRefresh } from "../../hooks/useRefresh";
import type {
  ApiId,
  CatalogResponse,
  Maintainer,
  MaintainerMember,
  MaintainerMemberPayload,
  MeResponse,
  NewMaintainerPayload,
  NewPackagePayload,
  NewServicePayload,
  Package,
  Service,
  UserSummary
} from "../../types/api";
import { showError } from "../../utils/notifications";
import {
  CatalogManagementModal,
  getDialogTitle,
  RemoveMemberModal
} from "./CatalogForms";
import type { CatalogDialog } from "./CatalogForms";

export function CatalogView({ client, user }: { client: ApiClient; user: MeResponse }) {
  const [dialog, setDialog] = useState<CatalogDialog>(null);
  const [selectedMaintainerId, setSelectedMaintainerId] = useState("");
  const [removingMember, setRemovingMember] = useState<MaintainerMember | null>(null);
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [data, actions] = useAsyncData<CatalogResponse>(async () => {
    const [maintainers, services, packages, users] = await Promise.all([
      client.get<Maintainer[]>("/maintainers"),
      client.get<Service[]>("/services"),
      client.get<Package[]>("/packages"),
      user.capabilities.view_user_directory
        ? client.get<UserSummary[]>("/users").catch(() => [])
        : Promise.resolve([])
    ]);
    return { maintainers, services, packages, users };
  }, [client]);
  const [membersData, memberActions] = useAsyncData<MaintainerMember[]>(
    async () =>
      selectedMaintainerId
        ? client.get<MaintainerMember[]>(
            `/maintainers/${encodeURIComponent(selectedMaintainerId)}/members`
          )
        : [],
    [client, selectedMaintainerId]
  );

  useRefresh(actions.reload);
  useRefresh(memberActions.reload);

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
    const lookup = new Map<ApiId, Maintainer>();
    (catalog?.maintainers || []).forEach((maintainer) => lookup.set(maintainer.id, maintainer));
    return lookup;
  }, [catalog?.maintainers]);
  const writableMaintainerIds = useMemo(
    () =>
      new Set(
        user.maintainer_access
          .filter((access) => access.can_write)
          .map((access) => access.maintainer_id)
      ),
    [user.maintainer_access]
  );
  const memberMaintainerIds = useMemo(
    () =>
      new Set(
        user.maintainer_access
          .filter((access) => access.can_manage_members)
          .map((access) => access.maintainer_id)
      ),
    [user.maintainer_access]
  );
  const canWriteMaintainer = (maintainerId: ApiId) =>
    user.capabilities.manage_maintainers || writableMaintainerIds.has(maintainerId);
  const canManageMaintainerMembers = (maintainerId: ApiId) =>
    user.capabilities.manage_maintainers || memberMaintainerIds.has(maintainerId);
  const writableMaintainerOptions = useMemo(
    () =>
      maintainerOptions.filter(
        (option) =>
          user.capabilities.manage_maintainers || writableMaintainerIds.has(Number(option.value))
      ),
    [maintainerOptions, user.capabilities.manage_maintainers, writableMaintainerIds]
  );
  const memberMaintainerOptions = useMemo(
    () =>
      maintainerOptions.filter(
        (option) =>
          user.capabilities.manage_maintainers || memberMaintainerIds.has(Number(option.value))
      ),
    [maintainerOptions, memberMaintainerIds, user.capabilities.manage_maintainers]
  );
  const userOptions = useMemo(
    () =>
      (catalog?.users || []).map((user) => ({
        value: String(user.id),
        label: `${user.username} (#${user.id})`
      })),
    [catalog?.users]
  );
  const userById = useMemo(() => {
    const lookup = new Map<ApiId, UserSummary>();
    (catalog?.users || []).forEach((user) => lookup.set(user.id, user));
    return lookup;
  }, [catalog?.users]);
  const defaultMaintainerId = writableMaintainerOptions[0]?.value || "";
  const defaultMemberMaintainerId = memberMaintainerOptions[0]?.value || "";
  const selectedMaintainer = maintainerById.get(Number(selectedMaintainerId)) || null;
  const canManageOwnedRecords = writableMaintainerOptions.length > 0;
  const canManageMembers = Boolean(
    selectedMaintainer &&
      canManageMaintainerMembers(selectedMaintainer.id) &&
      userOptions.length > 0
  );
  const dialogTitle = getDialogTitle(dialog);
  const removingMemberUser = removingMember ? userById.get(removingMember.user_id) : undefined;
  const removingMemberMaintainer = removingMember
    ? maintainerById.get(removingMember.maintainer_id)
    : undefined;

  useEffect(() => {
    if (!catalog) {
      return;
    }

    const selectedStillExists = memberMaintainerOptions.some(
      (maintainer) => maintainer.value === selectedMaintainerId
    );
    if (!selectedStillExists) {
      setSelectedMaintainerId(defaultMemberMaintainerId);
    }
  }, [catalog, defaultMemberMaintainerId, memberMaintainerOptions, selectedMaintainerId]);

  async function saveMaintainer(payload: NewMaintainerPayload) {
    if (
      !dialog ||
      dialog.type !== "maintainer" ||
      !user.capabilities.manage_maintainers
    ) {
      return;
    }

    setSaving(true);
    try {
      const maintainer = dialog.record
        ? await client.put<Maintainer>(`/maintainers/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post<Maintainer>("/maintainers", payload);
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

  async function saveService(payload: NewServicePayload) {
    if (
      !dialog ||
      dialog.type !== "service" ||
      !canWriteMaintainer(payload.maintainer_id) ||
      (dialog.record && !canWriteMaintainer(dialog.record.maintainer_id))
    ) {
      return;
    }

    setSaving(true);
    try {
      const service = dialog.record
        ? await client.put<Service>(`/services/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post<Service>("/services", payload);
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

  async function savePackage(payload: NewPackagePayload) {
    if (
      !dialog ||
      dialog.type !== "package" ||
      !canWriteMaintainer(payload.maintainer_id) ||
      (dialog.record && !canWriteMaintainer(dialog.record.maintainer_id))
    ) {
      return;
    }

    setSaving(true);
    try {
      const packageRecord = dialog.record
        ? await client.put<Package>(`/packages/${encodeURIComponent(dialog.record.id)}`, payload)
        : await client.post<Package>("/packages", payload);
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

  async function saveMember(payload: MaintainerMemberPayload) {
    if (
      !dialog ||
      dialog.type !== "member" ||
      !canManageMaintainerMembers(dialog.maintainerId)
    ) {
      return;
    }

    setSaving(true);
    try {
      const member = await client.post<MaintainerMember>(
        `/maintainers/${encodeURIComponent(dialog.maintainerId)}/members`,
        payload
      );
      await memberActions.reload();
      setDialog(null);
      notifications.show({
        title: dialog.record ? "Member role updated" : "Member added",
        message: `${userById.get(member.user_id)?.username || `User ${member.user_id}`} - ${
          member.role
        }`,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setSaving(false);
    }
  }

  async function removeMember(member: MaintainerMember) {
    if (!canManageMaintainerMembers(member.maintainer_id)) {
      return;
    }

    setRemoving(true);
    try {
      await client.delete(
        `/maintainers/${encodeURIComponent(member.maintainer_id)}/members/${encodeURIComponent(
          member.user_id
        )}`
      );
      await memberActions.reload();
      setRemovingMember(null);
      notifications.show({
        title: "Member removed",
        message: userById.get(member.user_id)?.username || `User ${member.user_id}`,
        color: "teal"
      });
    } catch (error) {
      showError(error);
    } finally {
      setRemoving(false);
    }
  }

  const MaintainerActionCell = ({ row }: { row: Maintainer }) => {
    const canManageMembersForRow = canManageMaintainerMembers(row.id);
    if (!canManageMembersForRow && !user.capabilities.manage_maintainers) {
      return null;
    }

    return (
      <Group gap="xs" wrap="nowrap">
        {canManageMembersForRow && (
          <Button
            variant="subtle"
            size="compact-sm"
            leftSection={<IconUsers size={16} />}
            onClick={() => setSelectedMaintainerId(String(row.id))}
          >
            Members
          </Button>
        )}
        {user.capabilities.manage_maintainers && (
          <EditAction
            label={`Edit maintainer ${row.display_name}`}
            onClick={() => setDialog({ type: "maintainer", record: row })}
          />
        )}
      </Group>
    );
  };
  const ServiceActionCell = ({ row }: { row: Service }) =>
    canWriteMaintainer(row.maintainer_id) ? (
      <EditAction
        label={`Edit service ${row.name}`}
        onClick={() => setDialog({ type: "service", record: row })}
      />
    ) : null;
  const PackageActionCell = ({ row }: { row: Package }) =>
    canWriteMaintainer(row.maintainer_id) ? (
      <EditAction
        label={`Edit package ${row.name}`}
        onClick={() => setDialog({ type: "package", record: row })}
      />
    ) : null;
  const MaintainerCell = ({ value }: { value?: unknown }) => {
    const maintainer = maintainerById.get(Number(value));
    return <Text size="sm">{maintainer?.display_name || "-"}</Text>;
  };
  const MemberUserCell = ({ value }: { value?: unknown }) => {
    const user = userById.get(Number(value));

    return (
      <Stack gap={0}>
        <Text size="sm" fw={700}>
          {user?.username || `User ${value}`}
        </Text>
        <Text size="xs" c="dimmed">
          #{String(value)}
          {user?.roles?.length ? ` - ${user.roles.join(", ")}` : ""}
        </Text>
      </Stack>
    );
  };
  const MemberActionCell = ({ row }: { row: MaintainerMember }) => (
    <Group gap="xs" wrap="nowrap">
      <Button
        variant="subtle"
        size="compact-sm"
        leftSection={<IconPencil size={16} />}
        onClick={() =>
          setDialog({ type: "member", maintainerId: row.maintainer_id, record: row })
        }
      >
        Role
      </Button>
      <Button
        color="red"
        variant="subtle"
        size="compact-sm"
        leftSection={<IconTrash size={16} />}
        onClick={() => setRemovingMember(row)}
      >
        Remove
      </Button>
    </Group>
  );

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
            maintainerOptions={writableMaintainerOptions}
            defaultMaintainerId={defaultMaintainerId}
            userOptions={userOptions}
            saving={saving}
            onSaveMaintainer={saveMaintainer}
            onSaveService={saveService}
            onSavePackage={savePackage}
            onSaveMember={saveMember}
          />

          <RemoveMemberModal
            member={removingMember}
            userLabel={
              removingMemberUser
                ? `${removingMemberUser.username} (#${removingMemberUser.id})`
                : removingMember
                  ? `User ${removingMember.user_id}`
                  : ""
            }
            maintainerLabel={removingMemberMaintainer?.display_name || ""}
            removing={removing}
            onCancel={() => setRemovingMember(null)}
            onConfirm={removeMember}
          />

          <Stack gap="lg">
            <DataPanel
              title="Maintainers"
              actions={
                user.capabilities.manage_maintainers ? (
                  <Button
                    leftSection={<IconPlus size={16} />}
                    size="compact-sm"
                    onClick={() => setDialog({ type: "maintainer" })}
                  >
                    New maintainer
                  </Button>
                ) : null
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

            {memberMaintainerOptions.length > 0 && (
              <DataPanel
                title="Maintainer members"
                actions={
                  <Button
                    leftSection={<IconUserPlus size={16} />}
                    size="compact-sm"
                    disabled={!canManageMembers}
                    onClick={() =>
                      selectedMaintainer &&
                      setDialog({ type: "member", maintainerId: selectedMaintainer.id })
                    }
                  >
                    Add member
                  </Button>
                }
              >
                <Stack gap="md">
                  <Select
                    label="Maintainer"
                    data={memberMaintainerOptions}
                    value={selectedMaintainerId}
                    onChange={(value) => setSelectedMaintainerId(value || "")}
                    searchable
                  />
                  {membersData.error && (
                    <Text size="sm" c="red">
                      {membersData.error.message}
                    </Text>
                  )}
                  {membersData.loading && !membersData.value ? (
                    <Text size="sm" c="dimmed">
                      Loading members...
                    </Text>
                  ) : (
                    <DataTable
                      rows={membersData.value || []}
                      columns={[
                        ["user_id", "User", MemberUserCell],
                        ["role", "Role", StatusBadge],
                        ["created_at", "Added", DateCell],
                        ["_actions", "Actions", MemberActionCell]
                      ]}
                    />
                  )}
                </Stack>
              </DataPanel>
            )}

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

function EditAction({ label, onClick }: { label: string; onClick: () => void }) {
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

function SourceCell({ value }: { value?: unknown }) {
  if (!value) {
    return null;
  }

  return (
    <Text size="sm" className="catalogSourceCell" title={String(value)}>
      {String(value)}
    </Text>
  );
}
