    # SigmaOS-plugin-marketplace

    Applet/desklet/extension marketplace

    Part of the **SigmaOS** sovereign operating-system ecosystem — a multi-repository
    distribution modelled on Linux Mint and Omarchy Linux. See the parent
    [`SigmaOS`](https://github.com/AaryanSinghChauhan09/SigmaOS) repository for the
    full component index.

    ## Upstream inspirations
    - https://github.com/linuxmint/cinnamon-spices-applets
- https://github.com/omacom/omarchy-plugin-marketplace

    ## Design principles applied
    - **Abstraction**
- **Plan Before Act**
- **Fail-Fast**
- **Defensive Programming**
- **KISS**
- **DRY**
- **Deterministic Testing**

    ## Usage
    Verbs: `search, install-plan, enable, list`

    ```
    $ sigma-plugin-market search clock --type applet
clock@sigmaos  Sigma Clock  3.2.1
$ sigma-plugin-market install-plan clock@sigmaos --cinnamon 6.4 \
    --installed clock@sigmaos=3.2.1
status: already-installed
$ sigma-plugin-market enable clock@sigmaos --store enabled.json --installed clock@sigmaos=3.2.1
$ sigma-plugin-market list --store enabled.json

    ```

    ## Build & test
    ```
    make build
    make test
    ```

    ## Licence
    GNU General Public License v3.0 (FOSS).
