import { createI18n } from 'vue-i18n'
import en from './locales/en.json'

export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages: { en },
})

/**
 * Map a server error code to an i18n key, falling back to a generic one.
 *
 * Tested against the source catalogue rather than a hand-kept list of codes:
 * that list silently fell behind the server and left `errors.room_locked`
 * translated but unreachable. Writing the message is now the whole job of
 * supporting a new code.
 *
 * Deliberately not `te()`, which only consults the ACTIVE locale — a code a
 * future translation had not covered yet would fall to the generic message
 * instead of vue-i18n's own fallback to the English one.
 */
export function errorKey(code: string): string {
  return code in en.errors ? `errors.${code}` : 'errors.generic'
}
