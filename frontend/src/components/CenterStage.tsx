import { Box } from "@mantine/core";

export function CenterStage({ children }) {
  return (
    <Box className="loginBackground">
      <Box className="loginContent">{children}</Box>
    </Box>
  );
}
