# Table source metadata

Pipe tables use `header-rows` and `footer-rows` for their leading and trailing
sections. `body-rows` gives the data-row count of each body, in order.
`body-header-rows` gives each body's intermediate header count, and
`body-header-cols` gives each body's leading row-header-column count.

```carve
{header-rows=1 body-rows=1,1 body-header-rows=1,0 body-header-cols=1,0 footer-rows=1}
| Name | Value |
| Group | Unit |
| Alpha | 1 |
| Beta | 2 |
| Total | 3 |
```

Both body header lists require `body-rows` and must match its length. Omitted
header-row counts default to zero only when the key is absent. Empty header-row
entries are invalid; empty header-column entries each occupy one position.
For zero bodies, omit both body header lists. Empty header-column entries leave the AST
field unset; `0` explicitly sets it to zero. `body-rows=""` states no bodies,
while `body-rows=0` states one empty body. All row counts must partition every
row exactly once. Invalid body metadata creates no explicit partition and all
row-group attributes remain ordinary HTML attributes.

Canonical export adds missing metadata attributes. It retains authored values
and diagnoses conflicts as `field-unspellable` for `rowGroups`. Table section
attributes and block cell content still have separate source limitations.
