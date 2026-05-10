import { notifications } from "@mantine/notifications";

export function showError(error: unknown) {
  notifications.show({
    title: "Request failed",
    message: error instanceof Error ? error.message : String(error),
    color: "red"
  });
}
