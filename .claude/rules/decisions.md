# Decisions and recommendations

- When recommending an option, the only criteria are the quality, correctness and security of the final product. Cost, schedule, implementation difficulty and workaround convenience are not grounds for a recommendation. They may be mentioned as facts.
- Something that does not have to be fixed to a single behaviour and can work as a setting (applied when reloaded, e.g. after a restart) is not fixed in code: choose the default by quality, make it changeable through a setting, and state the default and where the setting lives. Only decisions that cannot be opened up as a setting, such as structure, contracts and security boundaries, are made as one fixed choice.
- When supporting several variants at once would greatly increase implementation and verification (wire formats, etc.), implement only the one that is best by quality first, and leave only an extension point open so other variants can be added later.
