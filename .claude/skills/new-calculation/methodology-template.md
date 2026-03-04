# Methodology section template

Use this format when adding a new section to `docs/calculations/methodology.md`.

```markdown
### 4.X <Title> `#CALC_TAG`

<1-2 sentence description of what this calculation does and why.>

<Formulae in indented pseudocode, e.g.:>

    result = some_function(input_a, input_b)

<If there are multiple steps, number them:>

1. <Step one>
2. <Step two>

<Edge cases and error conditions:>

When <condition>, <what happens>.
<Error name> is returned when <situation>.

---
```

## Checklist

Before presenting to the user, verify:

- [ ] Tag follows `#CALC_*` pattern and is unique
- [ ] Formulae use the same pseudocode style as existing sections
- [ ] All inputs and outputs are named using existing domain types where possible
- [ ] Edge cases are explicitly stated (not left implicit)
- [ ] Cross-references to other `#CALC_*` tags are included where the calculation depends on them
- [ ] Section numbering follows sequentially from the last section
