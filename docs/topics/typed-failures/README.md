# Typed Function Failures

This shelf will cover source functions that declare the failures they can end
with, `expect` propagation, and intent-level `else` mapping, with their Core
and Target IR forms. The language specification
([Typed Function Failures](../../SPEC_edict-language-v1.md#typed-function-failures))
defines the feature. HEAD does not yet implement it: the parser still refuses
`fails` and `failure` declarations. The [test plan](./test-plan.md) lists the
planned verification.
