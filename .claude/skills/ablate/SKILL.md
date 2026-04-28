---
name: ablate
description: Design an ablation study
---

For "is this knob actually doing anything?" questions.

1. Identify the knob and the metric of interest.
2. Sketch the experiment matrix:
   - Baseline — knob disabled.
   - Knob enabled with default config.
   - 2-3 sweeps over the knob's parameter range.
3. Specify the dataset and measurement protocol:
   - Which clip(s)? Why those?
   - Which metric? Why that one?
   - What sample size? What variance is acceptable?
4. Persist the design as `docs/03.methodology/<slug>.md` before running.
5. Run the experiment, log results to `benchmarks/results/<date>-<slug>.md`.

Don't run the ablation without a written-up methodology — otherwise the result is unfalsifiable.
