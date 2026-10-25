import { describe, expect, test } from 'bun:test'
import { defaultProfile, defaultProxy, mappedAddresses, parsePortSpec } from './entities'
describe('proxy batch input', () => {
  test('normalizes whitespace, full-width commas, duplicate ports and ranges', () => {
    expect(parsePortSpec('9002,9000-9002，8080\n8080')).toEqual([8080,9000,9001,9002])
  })
  test('rejects invalid ports, descending ranges and more than 100 distinct ports', () => {
    for (const spec of ['0','65536','9002-9000','1e3','2.5','not-a-port']) expect(() => parsePortSpec(spec)).toThrow('invalid')
    expect(() => parsePortSpec('1-101')).toThrow('too_many')
    expect(parsePortSpec('1-100')).toHaveLength(100)
  })
})
test('mapped addresses use enabled bindings only and remove duplicate domains', () => {
  const proxy={...defaultProxy(),id:1,proxy_type:'https' as const,custom_domains:['example.com']}
  const profiles=[{...defaultProfile(),id:1},{...defaultProfile(),id:2}]
  const binding={id:1,profile_id:1,proxy_id:1,enabled:true,running:false,priority:0,created_at:'',updated_at:''}
  expect(mappedAddresses(proxy,profiles,[binding,{...binding,id:2,profile_id:2}])).toEqual(['https://example.com'])
  expect(mappedAddresses(proxy,profiles,[{...binding,enabled:false}])).toEqual([])
})
