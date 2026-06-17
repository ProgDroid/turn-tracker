import { createI18n } from 'vue-i18n'
import en from './locales/en.json'

export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages: { en },
})

/** Map a server error code to an i18n key, falling back to a generic one. */
export function errorKey(code: string): string {
  const known = [
    'not_authorized', 'not_found', 'wrong_state', 'not_your_turn', 'nudge_cooldown',
    'room_full', 'bad_name',
  ]
  return known.includes(code) ? `errors.${code}` : 'errors.generic'
}
