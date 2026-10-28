import type { QueryClient } from "@tanstack/react-query";

export async function invalidatePublicLetterings(
  client: QueryClient,
): Promise<void> {
  await Promise.all([
    client.invalidateQueries({ queryKey: ["letterings-infinite"] }),
    client.invalidateQueries({ queryKey: ["map"] }),
  ]);
}
