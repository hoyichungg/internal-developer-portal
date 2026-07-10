import { Alert, Button, Group, Select, Stack, Text, TextInput } from "@mantine/core";
import { IconPlugConnected } from "@tabler/icons-react";
import { useState } from "react";
import type { FormEvent } from "react";

import type { Maintainer, UserSummary } from "../../types/api";

export function ConnectorCreateForm({
  onCreate,
  onCancel,
  submitting,
  maintainers,
  users,
  scopeOptionsLoading,
  scopeOptionsError
}: {
  onCreate: (event: FormEvent<HTMLFormElement>) => void | Promise<void>;
  onCancel: () => void;
  submitting: boolean;
  maintainers: Maintainer[];
  users: UserSummary[];
  scopeOptionsLoading: boolean;
  scopeOptionsError: string | null;
}) {
  const [scopeType, setScopeType] = useState<string | null>(null);
  const [maintainerId, setMaintainerId] = useState<string | null>(null);
  const [ownerUserId, setOwnerUserId] = useState<string | null>(null);
  const [status, setStatus] = useState("active");

  function changeScope(nextScope: string | null) {
    setScopeType(nextScope);
    setMaintainerId(null);
    setOwnerUserId(null);
  }

  return (
    <form onSubmit={onCreate}>
      <Stack>
        <TextInput name="source" label="Source" placeholder="azure-devops" required />
        <TextInput name="kind" label="Kind" placeholder="azure_devops" required />
        <TextInput name="display_name" label="Display name" placeholder="Azure DevOps" required />
        <Select
          label="Imported data visibility"
          description="This applies to work cards, notifications, and run history produced by this connector."
          placeholder="Choose a visibility scope"
          value={scopeType}
          onChange={changeScope}
          data={[
            { value: "maintainer", label: "One maintainer team" },
            { value: "user", label: "One user only" },
            { value: "global", label: "Everyone in the portal" }
          ]}
          required
        />
        <input type="hidden" name="scope_type" value={scopeType || ""} />
        {scopeType === "maintainer" && (
          <>
            <Select
              label="Maintainer team"
              placeholder={scopeOptionsLoading ? "Loading teams..." : "Choose a team"}
              data={maintainers.map((maintainer) => ({
                value: String(maintainer.id),
                label: maintainer.display_name
              }))}
              value={maintainerId}
              onChange={setMaintainerId}
              searchable
              disabled={scopeOptionsLoading || Boolean(scopeOptionsError)}
              required
            />
            <input type="hidden" name="maintainer_id" value={maintainerId || ""} />
          </>
        )}
        {scopeType === "user" && (
          <>
            <Select
              label="Portal user"
              placeholder={scopeOptionsLoading ? "Loading users..." : "Choose a user"}
              data={users.map((user) => ({
                value: String(user.id),
                label: user.username
              }))}
              value={ownerUserId}
              onChange={setOwnerUserId}
              searchable
              disabled={scopeOptionsLoading || Boolean(scopeOptionsError)}
              required
            />
            <input type="hidden" name="owner_user_id" value={ownerUserId || ""} />
          </>
        )}
        {scopeType === "global" && (
          <Text size="xs" c="orange">
            Everyone with portal access will be able to see records imported by this connector.
          </Text>
        )}
        {scopeOptionsError && scopeType !== "global" && (
          <Alert color="red" title="Visibility options unavailable">
            {scopeOptionsError} Close this dialog and try again, or choose global visibility only if
            the imported data is safe for everyone.
          </Alert>
        )}
        <Select
          label="Status"
          value={status}
          onChange={(value) => setStatus(value || "active")}
          data={["active", "paused", "error"]}
        />
        <input type="hidden" name="status" value={status} />
        <Group justify="flex-end" mt="sm">
          <Button variant="default" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" loading={submitting} leftSection={<IconPlugConnected size={16} />}>
            Create
          </Button>
        </Group>
      </Stack>
    </form>
  );
}
