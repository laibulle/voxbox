Project Reference Bibliography: Virtual Analog & WDF Modeling
This document provides an overview of the foundational literature and state-of-the-art papers utilized in the development of this real-time DSP modeling engine.

1. The Core Architecture Blueprint
File: comj.2009.33.2.85.pdf

Citation: Pakarinen, J., & Yeh, D. T. (2009). A Review of Digital Techniques for Modeling Guitar Amplifiers. Computer Music Journal, 33(2), 85-98.

What it refers to: This is the definitive foundational review for Gray-Box modeling. It outlines how to break down complex tube amplifier topologies into discrete, cascaded stages—separating linear filtering (like tone stacks) from static non-linearities (like triode saturation). It provides the exact structural logic used to organize our pipeline.

2. The Wave Digital Filter Bible
File: Wave Digital Filters Theory and Practice.pdf

Citation: Fettweis, A. (1986). Wave Digital Filters: Theory and Practice. Proceedings of the IEEE, 74(2), 270-327.

What it refers to: The absolute "Genesis" of Wave Digital Filters (WDF). Fettweis explains how to map classical analog circuit variables (voltages and currents) onto wave variables (incident and reflected waves). This methodology allows the creation of digital structures that naturally mimic physical components while maintaining absolute numerical passivity and stability.

3. Solving Complex Topologies & Feedback
File: DAFx-15_submission_53.pdf

Citation: Werner, K. J., Smith, W. R., & Abel, J. S. (2015). Wave Digital Filter Adaptors for Arbitrary Topologies and Multiport Linear Elements. In Proceedings of the 18th International Conference on Digital Audio Effects (DAFx-15).

What it refers to: Historically, WDFs struggled heavily with circuits that couldn't be neatly arranged into simple series or parallel branches. Werner et al. solved this by deriving custom adaptors using Modified Nodal Analysis (MNA). This paper is what allows modern WDF libraries to compute complex, interconnected sub-circuits (like multi-port transformers or interactive tone stacks) without getting stuck in non-computable delay-free loops.

4. The Deep Learning Black-Box Baseline
File: 1804.07145v1.pdf

Citation: Schmitz, T., & Embrechts, J. J. (2018). Real Time Emulation of Parametric Guitar Tube Amplifier With Long Short Term Memory Neural Network. arXiv preprint arXiv:1804.07145.

What it refers to: This paper represents the pure Black-Box AI modeling methodology. It investigates using Recurrent Neural Networks (specifically LSTMs) to predict the output of a tube amplifier stage directly from raw data, including adapting to knob parameter changes (like gain). It serves as a benchmark for pure data-driven approaches vs. our physical structural modeling.

5. The Cutting Edge: Deep Gray-Box (Neural WDF)
File: DAFx24_paper_45.pdf

Citation: Massi, O., Manino, E., & Bernardini, A. (2024). Wave Digital Modeling of Circuits with Multiple One-Port Nonlinearities Based on Lipschitz-Bounded Neural Networks. In Proceedings of the 27th International Conference on Digital Audio Effects (DAFx-24).

What it refers to: The frontier of virtual analog. Solving multiple non-linear components simultaneously in WDF normally requires heavy iterative solvers (like Newton-Raphson), which completely destroy real-time mobile CPU budgets. This paper replaces those costly mathematical equations with miniature Lipschitz-Bounded Neural Networks. By mathematically bounding the network's constraints, they guarantee that the neural network will never cause the audio filter to explode, allowing ultra-fast execution of complex distortion stages.