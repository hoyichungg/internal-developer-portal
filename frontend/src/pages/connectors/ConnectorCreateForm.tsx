import { Button, Group, Select, Stack, TextInput } from "@mantine/core";
import { IconPlugConnected } from "@tabler/icons-react";

export function ConnectorCreateForm({ onCreate, onCancel, submitting }) {
  return (
    <form onSubmit={onCreate}>
      <Stack>
        <TextInput name="source" label="Source" placeholder="azure-devops" required />
        <TextInput name="kind" label="Kind" placeholder="azure_devops" required />
        <TextInput name="display_name" label="Display name" placeholder="Azure DevOps" required />
        <Select
          name="status"
          label="Status"
          defaultValue="active"
          data={["active", "paused", "error"]}
        />
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
