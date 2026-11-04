/** A bounded, presentation-only snapshot from the trusted notification host. */
export type Notice = {
  id: number;
  app: string;
  summary: string;
  body: string;
  critical: boolean;
  actions: { key: string; label: string }[];
};
