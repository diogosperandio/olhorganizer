import { ptBR, type MessageKey } from "./pt-BR";

export type { MessageKey };

/** BCP 47 tag of the active language, for dates and relative times. */
export const locale = "pt-BR";

const messages: Record<MessageKey, string> = ptBR;

/** The text for `key`, with "{name}" placeholders filled from `params`. */
export function t(key: MessageKey, params: Record<string, string | number> = {}): string {
  return messages[key].replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}
