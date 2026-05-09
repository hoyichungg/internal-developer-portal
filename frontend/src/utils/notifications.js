import { notifications } from "@mantine/notifications";

export function showError(error) {
  notifications.show({
    title: "Request failed",
    message: error.message,
    color: "red"
  });
}
