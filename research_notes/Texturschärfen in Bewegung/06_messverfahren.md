# Objective quality metrics for image quality in motion (texture shimmer under camera flight)

**Access note, applies to every section.** This session's network allowed `github.com` and web search only. `arxiv.org`, `ar5iv.labs.arxiv.org`, `netflixtechblog.com`, `www.cl.cam.ac.uk`, `videoprocessing.github.io`, the LIVE lab pages, ACM and IEEE could not be fetched (DNS refused). So every finding carries one of two access tags:

- **[OPENED]**: I fetched and read the page (GitHub READMEs and docs).
- **[SNIPPET]**: I saw only the search-engine excerpt of the source. The paper itself was NOT opened. Treat these as leads that need checking against the PDF before they are quoted in a report. The numbers in them are the least reliable part.

Evidence labels: MEASURED/VALIDATED, AUTHOR CLAIM, VENDOR CLAIM, OPINION.

No source was invented. Every repository below was fetched and exists: `Netflix/vmaf`, `gfxdisp/FovVideoVDP`, `gfxdisp/ColorVideoVDP`, `NVlabs/flip`, `IntelLabs/CGVQM`, `thunil/TecoGAN`.

## VMAF: features, behaviour under sharpening, flicker sensitivity, tooling

### Takeaway
VMAF is the wrong primary metric here. Its only temporal input is a mean absolute luma difference between adjacent frames, it is scoped by Netflix to compression and scaling artefacts, and it is documented to give credit for enhancement (sharpening), which is exactly the variable under test. VMAF NEG reduces that credit but is reported as still gameable. Use it at most as a secondary column.

### Cited Findings
- [OPENED] VENDOR CLAIM. VMAF's elementary features: VIF ("a modified version of VIF where the loss of fidelity in each scale is included as an elementary metric", four scales `vif_scale0..3`); ADM, formerly DLM, of which "only the DLM part is added as an elementary metric"; and Motion2, "a simple measure of the temporal difference between adjacent frames", computed by "calculating the average absolute pixel difference for the luminance component". No other temporal feature is described. — [Netflix/vmaf features.md](https://github.com/Netflix/vmaf/blob/master/resource/doc/features.md)
- [OPENED] VENDOR CLAIM. Scope: VMAF targets adaptive streaming and considers "compression artifact and scaling artifact" only; other artefact types (the FAQ names packet loss and transmission errors) may be predicted inaccurately. The FAQ does not claim that VMAF captures temporal artefacts. — [Netflix/vmaf faq.md](https://github.com/Netflix/vmaf/blob/master/resource/doc/faq.md)
- [OPENED] VENDOR CLAIM. Viewing geometry is baked into the model, not a parameter: the default model predicts quality on a 1080p HDTV at 3H viewing distance; the 4K model assumes 1.5H. Scoring a lower-resolution video implicitly models a larger viewing distance and inflates scores. Self-comparison is not guaranteed to give exactly 100. Default pooling is the arithmetic mean of per-frame scores; `--pool` offers harmonic mean, median, min, percentiles. — [Netflix/vmaf faq.md](https://github.com/Netflix/vmaf/blob/master/resource/doc/faq.md)
- [OPENED] VENDOR CLAIM. NEG ("No Enhancement Gain") mode exists because VMAF credits image enhancement; for encoder comparison it is desirable to "measure the gain achievable from compression without taking into account the gain from image enhancement". Selected by model files with the `neg` suffix. The page does not explain the mechanism; it links a tech memo and slides that I did not open. — [Netflix/vmaf models_v0.md](https://github.com/Netflix/vmaf/blob/master/resource/doc/models_v0.md)
- [OPENED] Licence and tooling: BSD+Patent (changed from Apache 2.0 on 2020-02-27). `libvmaf` C library with stand-alone `vmaf` CLI, Python library, FFmpeg integration (`--enable-libvmaf`), Dockerfile. CPU only, x86 SIMD (AVX2, AVX-512). libvmaf v3.0.0 dated 2023-12-07; the README news lists a "VMAF v1 model set" (2026-06) with an HDR version added 2026-10-26, so the repo is active. Also implements PSNR, PSNR-HVS, SSIM, MS-SSIM, CIEDE2000 and CAMBI (banding). — [Netflix/vmaf README](https://github.com/Netflix/vmaf)
- [SNIPPET] MEASURED (per the excerpt). Siniukov, Antsiferova, Kulikov, Vatolin, "Hacking VMAF and VMAF NEG: vulnerability to different preprocessing methods", 2021, arXiv 2107.04510: gamma correction combined with unsharp masking, and CLAHE tuned for NEG, raise the scores; the project page reports VMAF rising by 181.22% after preprocessing while visual quality dropped, and VMAF NEG rising by 13.66% with no visible change. — [arXiv 2107.04510](https://arxiv.org/abs/2107.04510v1), [project page](https://videoprocessing.github.io/hacking-vmaf-and-vmaf-neg)
- [SNIPPET] MEASURED (per the excerpt). Zvezdakova, Zvezdakov, Kulikov, Vatolin, "Hacking VMAF with Video Color and Contrast Distortion", 2019 (CEUR-WS Vol-2485): unsharp masking and histogram equalisation raised VMAF by roughly 5 to 6%; unsharp masking helped most on some clips and not on others. — [arXiv 1907.04807](https://arxiv.org/abs/1907.04807), [CEUR PDF](https://ceur-ws.org/Vol-2485/paper12.pdf)
- [SNIPPET] AUTHOR CLAIM. A 2026 preprint "On Optimizing Image Codecs for VMAF NEG" states NEG remains vulnerable when used to fine-tune codecs. — [arXiv 2602.21336](https://arxiv.org/html/2602.21336v1)
- [SNIPPET] AUTHOR CLAIM. A survey excerpt states VMAF "used the average frame difference as the only temporal feature, which highly related to video content rather than the temporal distortion"; ST-VMAF was proposed to address this. — [arXiv 2112.12284](https://arxiv.org/pdf/2112.12284)

### Inferences
- The Motion2 feature is described as a plain adjacent-frame luma difference. In a camera flight that number is dominated by the camera motion itself, which is identical across all texture variants. Shimmer adds a small term on top. VMAF therefore has no mechanism that isolates flicker; this follows from the feature definition, I found no experiment testing it.
- The benchmark's reference is a supersampled render from a sharper source. VIF and DLM both score how much reference detail survives, so a sharpened variant will tend to score higher whether or not it shimmers. NEG clips that gain but was built for codec comparison, not for rendering.
- Fixed 3H/1080p geometry does not match a sim cockpit (monitor at roughly 1H or less). The VDP family lets you state the geometry.

### Gaps
- The Netflix tech blog "Toward a better quality metric for the video community" (2020) could not be opened (host blocked). The NEG mechanism (clipping of enhancement gain in VIF and DLM) is therefore unsourced here and is not asserted.
- Li et al. 2016 blog and the VMAF papers: not opened.
- Whether Motion2 is computed on the reference only and on low-pass filtered luma: the features page does not say. Not asserted.
- No source found, opened or snippet, that tests VMAF on texture shimmer or temporal aliasing in rendered content.

## FovVideoVDP and ColorVideoVDP

### Takeaway
ColorVideoVDP is the best-supported general-purpose candidate: full reference, spatio-temporal contrast sensitivity, explicit display geometry, JOD score, per-pixel heatmap video and a per-band "distogram", MIT licence, pip-installable, accepts PNG frame sequences. FovVideoVDP is its achromatic predecessor and its README explicitly names flicker. Neither is shown, in what I could read, to be validated on mip/texture shimmer.

### Cited Findings
- [OPENED] AUTHOR CLAIM. ColorVideoVDP is "the first color-aware metric that accounts for spatial and temporal aspects of vision"; it "models spatio-temporal sensitivity so that it can predict visibility of artifacts like waveguide nonuniformity and other temporally varying artifacts". Chromatic and achromatic sensitivity come from castleCSF. The README does not use the word flicker. — [gfxdisp/ColorVideoVDP](https://github.com/gfxdisp/ColorVideoVDP)
- [OPENED] Practical: MIT licence. PyTorch; CUDA recommended, CPU works but is "much longer, especially for video"; Apple MPS supported. Needs ffmpeg and FreeImage. Install `pip install cvvdp` or `pip install cvvdp[cuda]`. Inputs: MP4 and other containers, PNG sequences with printf names (`frame_%05d.png` plus `--fps`), raw `.yuv`, PQ/HLG HDR video, OpenEXR. Latest release v0.5.7 dated 2 August 2026. — [gfxdisp/ColorVideoVDP](https://github.com/gfxdisp/ColorVideoVDP)
- [OPENED] Display model: resolution, viewing distance, diagonal, peak luminance, contrast and ambient light come from `display_models.json` (`standard_4k`, `standard_fhd`, custom files via `--config-paths`); the tool reports the resulting pixels per degree. — [gfxdisp/ColorVideoVDP](https://github.com/gfxdisp/ColorVideoVDP)
- [OPENED] Output: JOD units, 10 = no difference, can fall below 0; the README's worked example is that a 9 JOD result is preferred over an 8 JOD result by about 75% of viewers. `--heatmap` (`supra-threshold`, `threshold`, `raw`), `--distogram`, `--dump-channels` (`temporal`, `lpyr`, `difference`). `--temp-padding` (`replicate` default, `symmetric`), `--temp-resample`, `--frames`. — [ColorVideoVDP README](https://github.com/gfxdisp/ColorVideoVDP/blob/main/README.md)
- [OPENED] The repo also ships PU-PSNR and ColorVideoVDP-ML variants (`cvvdp-ml-saliency`, `cvvdp-ml-transformer`, added v0.5.0). The repo's examples include an `aliasing` folder with bicubic and nearest-neighbour resampling files (spatial resampling, not described as temporal aliasing). — [ColorVideoVDP README](https://github.com/gfxdisp/ColorVideoVDP/blob/main/README.md)
- [OPENED] AUTHOR CLAIM. FovVideoVDP models the response to changes over time and across the visual field and "can predict temporal artifacts like flicker and judder"; it accounts for peripheral acuity. Full reference, SDR and HDR, same `display_models.json` approach, JOD output, heatmaps. PyTorch and MATLAB versions. Install `pip install pyfvvdp`. Latest v1.2.2 dated 24/02/2025. The README links a LICENSE file but does not state the licence in the text I saw. — [gfxdisp/FovVideoVDP](https://github.com/gfxdisp/FovVideoVDP)
- [SNIPPET] Mantiuk, Denes, Chapiro et al., "FovVideoVDP: A visible difference predictor for wide field-of-view video", ACM TOG (SIGGRAPH) 2021. AUTHOR CLAIM per excerpt: derived from psychophysical models of spatio-temporal contrast sensitivity, cortical magnification and contrast masking. — [paper PDF](https://www.cl.cam.ac.uk/~rkm38/pdfs/mantiuk2021_VideoVDP.pdf)
- [SNIPPET] Mantiuk et al., "ColorVideoVDP: A visual difference predictor for image, video and display distortions", SIGGRAPH 2024 / ACM TOG, DOI 10.1145/3658144. Per excerpt: outputs JOD score, distortion-map video and distogram; validated on the new XR-DAVID dataset "as well as several datasets from the literature"; stated limitation: no higher-level saliency or annoyance model. — [arXiv 2401.11485](https://arxiv.org/abs/2401.11485v2)
- [SNIPPET] XR-DAVID: 14 reference videos at 1080p, 8 display-artefact distortions including spatiotemporal dithering, waveguide nonuniformity and colour fringes, pairwise comparison with 77 participants. — [Cambridge repository record](https://www.repository.cam.ac.uk/handle/1810/367736)
- [SNIPPET] Denes and Mantiuk, "Predicting visible flicker in temporally changing images", IS&T HVEI 2020, DOI 10.2352/ISSN.2470-1173.2020.11.HVEI-233: multi-scale analysis of the difference between consecutive frames, normalised by a spatio-temporal CSF (pyramid of visibility), probability summation across spatial bands, calibrated on user markings. Motivated by display flicker at 40 to 180 Hz. — [paper PDF](https://www.cl.cam.ac.uk/~rkm38/pdfs/denes2020flicker_model.pdf)
- [SNIPPET] AUTHOR CLAIM (Intel, competing metric). In the CGVQM paper's appendix, CGVQM variants score the highest average PLCC across three datasets "followed by ColorVideoVDP". — [arXiv 2506.11546](https://arxiv.org/pdf/2506.11546)

### Inferences
- The VDP metrics compare test against reference in spatio-temporal frequency bands. Shimmer is a temporal modulation present in the test and absent in a heavily supersampled reference, so it lands in the temporal channel difference. This is a model property, not a validated result for texture shimmer.
- The heatmap video and distogram are the feature most useful to this benchmark: they show where (distance band toward the horizon) and in which band a variant differs, which a single score hides.
- Sharpening is neither rewarded nor penalised by construction: the metric reports visible difference from the reference. If the reference comes from the zoom-18 source, a mildly sharpened level 0 that moves closer to it scores better, overshoot beyond it scores worse.
- The JOD number depends on the display model. The geometry must be fixed and reported (a sim monitor at about 60 to 70 cm, not `standard_fhd` defaults, is the honest choice).
- Frame rate matters: the temporal CSF is applied at the stated `--fps`. Render and score at the real display rate (60 Hz), not a reduced rate.

### Gaps
- Which datasets the two metrics were calibrated on, and their correlations: not readable (papers blocked). The READMEs name none.
- Whether any calibration dataset contains temporal aliasing, TAA or mip shimmer: unknown. XR-DAVID as excerpted contains display artefacts, not rendering aliasing. I found no evidence of validation on texture shimmer.
- FovVideoVDP licence text: not read.
- "LIVE-FB" as a rendering-artefact dataset: no such evidence found; not asserted.
- Whether the metric assumes fixation or smooth pursuit of moving content, which changes the visibility of flicker on moving ground: not established from what I read.

## FLIP and HDR-FLIP

### Takeaway
FLIP is a still-image difference evaluator and its repository has no video or temporal mode. It can be run per frame to give error maps, but it then measures per-frame error and is blind to flicker as such.

### Cited Findings
- [OPENED] NVlabs/flip implements LDR-FLIP and HDR-FLIP; BSD 3-Clause, copyright 2020-2025 NVIDIA; current version shown is v1.7; Python (3.10+, `pip install flip-evaluator`), C++ single header `FLIP.h`, CUDA backend, PyTorch loss module. Inputs EXR and PNG. The README makes no mention of video or temporal evaluation. — [NVlabs/flip](https://github.com/NVlabs/flip)
- [SNIPPET] AUTHOR CLAIM. Andersson, Nilsson, Akenine-Möller, Oskarsson, Åström, Fairchild, "FLIP: A Difference Evaluator for Alternating Images", HPG 2020 / PACMCGIT: produces a map approximating the difference perceived when alternating between two images, with focus on rendered images versus ground truth. — [NVIDIA Research page](https://research.nvidia.com/publication/flip)
- [SNIPPET] A targeted search for a video or temporal extension of FLIP returned nothing. — same search, no positive result

### Inferences
- Per-frame FLIP against the supersampled reference is a rendering-community-accepted error map and cheap to add, but its mean over frames cannot separate "constant blur" from "shimmer with the same mean error".
- A temporal statistic of the FLIP map (variance over time along reprojected positions) would be a home-made measure with no validation.

### Gaps
- The FLIP paper itself was not opened, so its viewing-geometry parameter (pixels per degree) and user-study validation are not cited here.
- No 2024 to 2026 temporal FLIP development found; absence of search hits is not proof of absence.

## Temporal SSIM variants, classic video metrics, warping-based consistency metrics

### Takeaway
The classic video metrics (ST-RRED, SpEED-QA, ST-VMAF, MOVIE, VQM) model temporal distortion through statistics of frame differences and were built on compression databases; I found no maintained, packaged implementation worth adopting and no flicker-specific validation. The practically used "temporal stability" numbers in the reconstruction literature are flow-warped errors (E_warp, tOF, tLP), which several authors themselves flag as rewarding blur.

### Cited Findings
- [OPENED] TecoGAN repo: Chu, Xie, Leal-Taixé, Thuerey, "Learning Temporal Coherence via Self-Supervision for GAN-based Video Generation", ACM TOG, DOI 10.1145/3386569.3392457. Apache-2.0. Evaluation mode `python3 runGan.py 2` computes PSNR, LPIPS and "our temporal metrics tOF and tLP". The README does not define them. 24 commits, 81 open issues. — [thunil/TecoGAN](https://github.com/thunil/TecoGAN)
- [SNIPPET] tOF compares optical flow estimated from the output with flow from the ground truth. — search excerpt of later papers, e.g. [arXiv 2312.16247](https://arxiv.org/pdf/2312.16247)
- [SNIPPET] AUTHOR CLAIM / OPINION in the literature. Warping error (previous output frame warped to the current by optical flow, then compared): one paper notes it can be lower for over-smoothed output; another that it is not zero even for ground-truth video; a "relational warping error" variant subtracts the ground-truth frame difference so natural variation is not penalised. — [arXiv 2201.11632](https://arxiv.org/pdf/2201.11632), [arXiv 2312.16247](https://arxiv.org/pdf/2312.16247), [arXiv 2403.06243](https://arxiv.org/pdf/2403.06243) (attribution of each statement to a specific paper among these is not verified)
- [SNIPPET] Bampis, Li, Bovik, "SpatioTemporal Feature Integration and Model Fusion for Full Reference Video Quality Assessment" (ST-VMAF): 12 features, 5 from VMAF, 3 from T-SpEED, 4 from T-VIF; the temporal ones use a GSM statistical model on frame differences. — [arXiv 1804.04813](https://arxiv.org/pdf/1804.04813)
- [SNIPPET] One table excerpt on the NFLX dataset: ST-VMAF about 0.927, VMAF 0.6.1 about 0.928, SpEED-QA 0.781 (correlation type not confirmed). — [arXiv 2103.07770](https://arxiv.org/pdf/2103.07770)
- [SNIPPET] ST-RRED (Soundararajan and Bovik) models temporal distortion with statistics of spatially band-passed frame differences. — [arXiv 2412.04508](https://arxiv.org/pdf/2412.04508)
- [OPENED] libvmaf ships SSIM, MS-SSIM, PSNR and PSNR-HVS as per-frame metrics. — [Netflix/vmaf README](https://github.com/Netflix/vmaf)

### Inferences
- Per-frame SSIM/MS-SSIM averaged over time has no temporal term. It is useful only as the motion counterpart of the developer's existing still benchmark.
- Flow-based metrics carry flow-estimation error. In this benchmark the motion field is known exactly from the camera and plane, so the estimator can be replaced by analytic reprojection; the main published weakness of E_warp (flow error) then disappears, while the second (blur scores well) remains and must be countered by reporting a fidelity number next to it.
- Frame-difference statistics (ST-RRED family) conflate camera motion and distortion in the same way as VMAF's motion feature unless test and reference differences are compared; they do compare the two, so in principle they respond to shimmer, but nothing I found measures that.

### Gaps
- MOVIE (Seshadrinathan and Bovik 2010), VQM (Pinson and Wolf 2004), SpEED-QA, ST-RRED primary papers: not opened. No repository verified for any of them, so none is listed.
- Definitions of tOF and tLP: not read from the primary paper.
- No source found that validates any warping-based metric against human judgments of shimmer.

## Rendering-specific metrics and flicker metrics

### Takeaway
CGVQM (Intel, 2025) exists, is open source and is the only metric found that is calibrated on real-time rendering artefacts; its dataset is about supersampling, denoising, frame generation and similar, not mip filtering. Flicker-specific perceptual work exists (Denes and Mantiuk 2020; LIVE Flicker), but I found no packaged tool from it and it targets display flicker or compression flicker.

### Cited Findings
- [OPENED] IntelLabs/CGVQM: full-reference video metric, "the first metric calibrated for distortions from advanced rendering techniques", covering spatial and temporal artefacts. Apache-2.0. PyTorch, CUDA preferred, CPU supported. Run by editing `demo_cgvqm()` in `cgvqm.py` (examples are MP4; dependencies `numpy scipy av`). Outputs error maps plus a score on a scale described from "imperceptible" to "annoying". `train.py` allows recalibration on own data. Dataset CGVQD released as v0.1.0. 17 commits. — [IntelLabs/CGVQM](https://github.com/IntelLabs/CGVQM)
- [SNIPPET] Jindal, Sadaka, Thomas, Sochenov, Kaplanyan, "CGVQM+D: Computer Graphics Video Quality Metric and Dataset", Computer Graphics Forum (High-Performance Graphics 2025), arXiv 2506.11546. Per excerpts: 80 video sequences; distortions from neural supersampling (FSR, XeSS, DLSS), novel-view synthesis / Gaussian splatting, path tracing, neural denoising, frame interpolation, variable rate shading; existing full-reference metrics reached at most Pearson 0.78 on it; metric built on 3D ResNet-18 features. An Intel figure caption names "aliasing, ghosting, flickering, and reconstruction artifacts". — [arXiv 2506.11546](https://arxiv.org/abs/2506.11546v1), [EG digital library](https://diglib.eg.org/handle/10.1111/cgf70221), [Intel article](https://www.intel.com/content/www/us/en/developer/articles/technical/cgvqm-d-computer-graphics-video-quality.html)
- [SNIPPET] LIVE Flicker Video Database (Choi, Cormack, Bovik; "Motion silencing of flicker distortions on naturalistic videos", Signal Processing: Image Communication 2015): built to study how object motion suppresses the visibility of flicker; later excerpt says silencing is a peripheral effect that does not occur near fixation; Choi and Bovik built a flicker-sensitive full-reference model from it. — [LIVE database page](https://www.colorado.edu/lab/live/live-flicker-video-database), [paper PDF](https://www.live.ece.utexas.edu/publications/2015/LarkKwonChoi_SPIC_2015_Motion_silencing_of_flicker_distortions_on_naturalistic_videos.pdf)
- [SNIPPET] Yang, Liu, Salvi, "A Survey of Temporal Antialiasing Techniques", Computer Graphics Forum 39 (2020), DOI 10.1111/cgf.14018. The abstract excerpt does not mention evaluation metrics. — [NVIDIA Research page](https://research.nvidia.com/labs/rtr/publication/yang2020survey)
- [SNIPPET] A 2026 preprint on neural image-space tessellation measures temporal stability as "motion-compensated luminance Flicker on consecutive frames". — [arXiv 2602.23754](https://arxiv.org/pdf/2602.23754)
- [SNIPPET] Later Mantiuk-group flicker work: Bozorgian, Ashraf, Mantiuk 2023 (CSF and critical flicker frequency); elaTCSF 2024 (temporal CSF for flicker detection, variable refresh rate). — [elaTCSF project page](https://www.cl.cam.ac.uk/research/rainbow/projects/elaTCSF/)

### Inferences
- CGVQM is a learned metric (3D CNN features) with 80 calibration clips. Mip-chain shimmer and unsharp-mask overshoot are outside its stated distortion list, so its numbers here would be an extrapolation. It is worth running as a second opinion next to ColorVideoVDP, not as the judge.
- A learned feature metric gives no interpretable link to viewing distance; ColorVideoVDP does.
- The LIVE motion-silencing result cuts in a relevant direction: flicker on fast-moving content is less visible, at least in the periphery. Signal-level flicker energy therefore overstates visibility for fast near-ground flow, which is a reason to keep a perceptual metric alongside it.

### Gaps
- CGVQM paper tables (per-metric correlations, including VMAF and FovVideoVDP): not read.
- CGVQM input limits (clip length, resolution, frame-sequence input) are not stated in the README.
- Fan et al. 2006 (H.264 intra flicker metric), Vo et al., Kellnhofer et al. 2015 "transformation-aware perceptual image metric", Denes et al. 2020 motion quality model, a Meta/Oculus flicker metric, and NVIDIA's own DLSS temporal-stability evaluation: none searched successfully or opened. Nothing is asserted about them. Existence of a public Meta/Oculus flicker metric is unverified.
- How the TAA survey evaluates temporal stability, and whether it says no standard metric exists: not established.
- No metric was found that is validated specifically for texture/mip shimmer under camera motion. That appears to be an open spot; state it as "not found", not as "does not exist".

## Butteraugli, SSIMULACRA2 and other still-image metrics

### Takeaway
Both are still-image metrics; video use means scoring each frame and averaging, with no temporal term. They continue the developer's still benchmark but say nothing about flicker.

### Cited Findings
- [SNIPPET] OPINION / tool documentation. A Rust crate's description states Butteraugli and SSIMULACRA are meant for images and the libjxl binaries only process stills; its video tool decodes each frame, scores it as a still and averages. — [lib.rs butter-video](https://lib.rs/crates/butter-video)
- [SNIPPET] OPINION. A community codec wiki says SSIMULACRA2 has no inter-frame temporal awareness; `ssimulacra2_rs` has a video mode. — [codec-wiki](https://codec-wiki.pages.dev/docs/metrics/ssimulacra2)
- [SNIPPET] Vship offers GPU-accelerated SSIMULACRA2, Butteraugli and CVVDP. — [Vship on Codeberg](https://codeberg.org/Unneverseen/Vship) (repository not opened, existence taken from the search result only)

### Inferences
- Per-frame Butteraugli/SSIMULACRA2 on rendered frames answers "how close is each frame to the supersampled reference", which is the fidelity axis. Paired with a flicker axis it gives the two-dimensional result this benchmark needs.

### Gaps
- No primary libjxl or SSIMULACRA2 documentation was opened to confirm the still-only statement; it rests on secondary tool descriptions.
- No temporal adaptation of either metric was found.

## Signal-level flicker measures without a perceptual model

### Takeaway
A motion-compensated temporal error is defensible and has precedent (warping error, "motion-compensated luminance flicker"), and this benchmark can compute it exactly because the motion is analytic. It has no psychophysical validation as a shimmer measure and it rewards blur, so it must be reported next to a fidelity number and normalised by the reference.

### Cited Findings
- [SNIPPET] Watson, Ahumada, Farrell, "Window of visibility: a psychophysical theory of fidelity in time-sampled visual motion displays", JOSA A 3(3), 1986: derives spatio-temporal spectra of moving images and how time sampling alters them; a sampled motion whose aliases fall outside the window of visibility appears smooth. — [Optica abstract](https://opg.optica.org/abstract.cfm?URI=josaa-3-3-300), [NASA record](https://ntrs.nasa.gov/citations/19860047713)
- [SNIPPET] Warping error and its caveats (lower for over-smoothed output; non-zero for ground truth; relational variant subtracting the ground-truth difference). — [arXiv 2201.11632](https://arxiv.org/pdf/2201.11632), [arXiv 2312.16247](https://arxiv.org/pdf/2312.16247)
- [SNIPPET] Motion-compensated luminance flicker on consecutive frames used as a temporal stability measure in a 2026 rendering preprint. — [arXiv 2602.23754](https://arxiv.org/pdf/2602.23754)
- [SNIPPET] Denes and Mantiuk 2020 build their perceptual flicker predictor on the consecutive-frame difference, then weight it by a spatio-temporal CSF. — [paper PDF](https://www.cl.cam.ac.uk/~rkm38/pdfs/denes2020flicker_model.pdf)

### Inferences
Proposed measures (my construction, grounded in the above, not taken from a paper):
- **Reprojected temporal error.** For each ground point visible in frames t and t+1, sample linear-light luminance at its exact projected position in both frames (the homography of the plane gives it without flow estimation) and take the difference. Report RMS, and split it by distance band, since shimmer concentrates toward the horizon.
- **Normalise by the reference.** The supersampled reference also changes along a trajectory, legitimately, because the footprint changes with distance. Report test minus reference (the "relational" form), or the ratio. This is the direct answer to the published objection that warping error is non-zero for ground truth.
- **Temporal high-pass energy along trajectories.** Over a window of N frames per ground point, take the temporal spectrum of luminance and report energy above a cut-off (for example above a few Hz) relative to the reference. This is the window-of-visibility idea applied without a CSF.
- **Pair with fidelity.** Blur minimises all three. Always plot flicker against per-frame fidelity (SSIM or Butteraugli to the reference) so that "less flicker because blurrier" is visible as such. The expected picture: sharpening every mip level moves right on fidelity near the camera and up on flicker in the distance; Kaiser mips change both.
- Work in linear light for the signal measures; the perceptual metrics take display-encoded frames plus a display model.
- The reference renderer must supersample in time-consistent fashion (same sample pattern each frame or enough samples that noise is negligible), otherwise reference noise sets the flicker floor.

### Gaps
- No source found that validates motion-compensated temporal error against human ratings of shimmer. The measure is defensible by construction and precedent, not by validation.
- Watson et al. 1986 was not opened; the shape and limits of the window are not cited here.
- The 2026 preprint's exact flicker formula was not read.

## Recommendation matrix

### Takeaway
Use three columns: (1) ColorVideoVDP with a stated cockpit display model as the perceptual judge, with heatmaps; (2) the reprojected temporal error relative to the reference as the transparent flicker number; (3) per-frame fidelity (the metrics already used on stills) as the control against blur. Add CGVQM as a second opinion if a GPU is available. Do not use VMAF as a judge.

### Cited Findings
Each row restates findings sourced above; the "suitability" column is my assessment (OPINION).

| Metric | Temporal model | Treats sharpening | Validated on rendering shimmer | Licence, form | Suitability |
|---|---|---|---|---|---|
| ColorVideoVDP | spatio-temporal CSF, colour, display geometry | neutral: difference from reference | not found | MIT, `pip install cvvdp`, PyTorch, GPU advised, PNG sequences — [repo](https://github.com/gfxdisp/ColorVideoVDP) | primary perceptual metric |
| FovVideoVDP | spatio-temporal, names flicker and judder, achromatic, foveation | neutral | not found | `pip install pyfvvdp`, PyTorch/MATLAB, licence text not read — [repo](https://github.com/gfxdisp/FovVideoVDP) | fallback; superseded in scope by ColorVideoVDP (my reading, the README does not say so) |
| CGVQM | learned 3D CNN features, calibrated on rendering artefacts | unknown | supersampling, denoising, frame generation; not mip shimmer | Apache-2.0, PyTorch script — [repo](https://github.com/IntelLabs/CGVQM) | second opinion |
| VMAF / VMAF NEG | mean adjacent-frame luma difference only | VMAF credits enhancement; NEG limits it, reported still gameable | no; scoped to compression and scaling | BSD+Patent, C, ffmpeg, CPU — [repo](https://github.com/Netflix/vmaf) | not as judge; optional column |
| FLIP per frame | none | neutral: difference from reference | no | BSD-3, `pip install flip-evaluator` — [repo](https://github.com/NVlabs/flip) | error maps only |
| SSIM, PSNR, Butteraugli, SSIMULACRA2 per frame | none | penalise any deviation from reference, including sharpening beyond it | no | various; SSIM/PSNR in libvmaf | fidelity control axis |
| tOF / tLP, E_warp | flow-warped frame differences | blur scores well | no human validation found | Apache-2.0 research code — [TecoGAN](https://github.com/thunil/TecoGAN) | replace with analytic reprojection |
| Reprojected temporal error (own) | exact, motion known | blur scores well; report against fidelity | none | own code, Rust or NumPy, no GPU | primary transparent flicker number |

### Inferences
- CI fit: the signal measure and per-frame SSIM/PSNR run on CPU in seconds and can gate. ColorVideoVDP on CPU is slow per its README; keep clips short (a few seconds at 60 Hz, modest resolution) or run it as a manual/GPU job.
- Report ColorVideoVDP results as JOD differences between variants under one fixed display model; absolute JOD depends on the reference being supersampled from a sharper source than any variant can reach.
- Since no metric here is validated for mip shimmer, a small sanity check is worth more than a fourth metric: include two variants with known ordering (for example a LOD bias of -1, which must shimmer more, and +1, which must shimmer less but lose fidelity) and confirm each metric ranks them correctly before trusting it on the sharpening variants.

### Gaps
- No head-to-head comparison of these metrics on texture shimmer was found; the ranking above is reasoning from each metric's design.
- Primary papers for every metric remain unread because of the network restriction. Hosts that would need to be allowed to close this: `arxiv.org`, `www.cl.cam.ac.uk`, `netflixtechblog.com`, `videoprocessing.github.io`, `research.nvidia.com`, `live.ece.utexas.edu`, `dl.acm.org`.
