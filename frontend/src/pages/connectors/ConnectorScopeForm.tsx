import { Alert, Button, Group, Select, Stack, Text } from "@mantine/core";
import { useState } from "react";
import type { FormEvent } from "react";

import type {
  Connector,
  ConnectorScopePayload,
  Maintainer,
  UserSummary
} from "../../types/api";

const scopeOptions = [
  { value: "global", label: "Everyone" },
  { value: "maintainer", label: "One maintainer team" },
  { value: "user", label: "One private user" }
];

export function ConnectorScopeForm({
  connector,
  maintainers,
  users,
  optionsLoading,
  optionsError,
  saving,
  onSave,
  onCancel
}: {
  connector: Connector;
  maintainers: Maintainer[];
  users: UserSummary[];
  optionsLoading: boolean;
  optionsError: string | null;
  saving: boolean;
  onSave: (payload: ConnectorScopePayload) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [scopeType, setScopeType] = useState<ConnectorScopePayload["scope_type"]>(
    connector.scope_type
  );
  const [maintainerId, setMaintainerId] = useState(
    connector.maintainer_id === null ? "" : String(connector.maintainer_id)
  );
  const [ownerUserId, setOwnerUserId] = useState(
    connector.owner_user_id === null ? "" : String(connector.owner_user_id)
  );

  const ownerMissing =
    (scopeType === "maintainer" && !maintainerId) || (scopeType === "user" && !ownerUserId);

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (ownerMissing) return;

    onSave({
      scope_type: scopeType,
      owner_user_id: scopeType === "user" ? Number(ownerUserId) : null,
      maintainer_id: scopeType === "maintainer" ? Number(maintainerId) : null
    });
  }

  return (
    <form onSubmit={submit}>
      <Stack gap="md">
        <Text size="sm" c="dimmed">
          This changes who can see the connector and moves all work cards and notifications
          imported by it to the same visibility in one transaction.
        </Text>
        {optionsError && (
          <Alert color="red" title="Visibility options unavailable">
            {optionsError}
          </Alert>
        )}
        <Select
          label="Visibility"
          data={scopeOptions}
          value={scopeType}
          allowDeselect={false}
          onChange={(value) =>
            setScopeType((value as ConnectorScopePayload["scope_type"]) || "global")
          }
        />
        {scopeType === "maintainer" && (
          <Select
            label="Maintainer team"
            placeholder="Choose a team"
            searchable
            data={maintainers.map((maintainer) => ({
              value: String(maintainer.id),
              label: `${maintainer.display_name} (${maintainer.email})`
            }))}
            value={maintainerId}
            onChange={(value) => setMaintainerId(value || "")}
            disabled={optionsLoading || Boolean(optionsError)}
            required
          />
        )}
        {scopeType === "user" && (
          <Select
            label="Private user"
            placeholder="Choose a user"
            searchable
            data={users.map((user) => ({
              value: String(user.id),
              label: user.username
            }))}
            value={ownerUserId}
            onChange={(value) => setOwnerUserId(value || "")}
            disabled={optionsLoading || Boolean(optionsError)}
            required
          />
        )}
        <Group justify="flex-end">
          <Button type="button" variant="default" onClick={onCancel} disabled={saving}>
            Cancel
          </Button>
          <Button type="submit" loading={saving} disabled={ownerMissing || Boolean(optionsError)}>
            Save visibility
          </Button>
        </Group>
      </Stack>
    </form>
  );
}
