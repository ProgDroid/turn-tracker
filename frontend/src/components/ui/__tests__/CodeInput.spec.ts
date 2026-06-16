import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import CodeInput from '@/components/ui/CodeInput.vue'

describe('CodeInput', () => {
  it('uppercases input and emits complete on 6 chars', async () => {
    const wrapper = mount(CodeInput)
    const input = wrapper.get('input')
    await input.setValue('gr7k9p')
    expect((wrapper.emitted('update:modelValue')?.at(-1)?.[0] as string)).toBe('GR7K9P')
    expect(wrapper.emitted('complete')).toBeTruthy()
  })

  it('does not emit complete below 6 chars', async () => {
    const wrapper = mount(CodeInput)
    await wrapper.get('input').setValue('gr7')
    expect(wrapper.emitted('complete')).toBeFalsy()
  })
})
