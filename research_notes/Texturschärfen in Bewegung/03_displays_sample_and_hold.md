# Sample-and-hold display motion blur vs. perceivable texture sharpness

> **ACCESS STATUS — read first.** In this session every direct page fetch failed (DNS blocked for
> hsi.arc.nasa.gov, ntrs.nasa.gov, www.cl.cam.ac.uk, engineering.purdue.edu, blurbusters.com,
> arxiv.org, wikipedia.org, crossref.org; curl through the proxy failed the same way). **No paper
> was opened in full text.** Everything under "Cited Findings" comes from **web-search result
> summaries/snippets** of the linked pages (abstract-level evidence). Each item is marked
> `[SNIPPET]`. Equations and thresholds that I could not confirm in a snippet are NOT attributed to
> a paper; they are given as textbook signal-processing derivations labelled **OWN CALCULATION**.
> The primary sources the brief named that I could **not verify at all** (not even at abstract
> level): Klompenhouwer & Velthoven 2004 (SPIE), Klompenhouwer 2006 (SID Digest), Kurita 2001
> (SID), Feng 2006 (synchronized backlight flashing; only a title hit), Chapiro et al. 2019,
> Watson/Ahumada/Farrell 1986 (only a one-sentence abstract snippet). The report writer should
> treat the paper-specific statements as "abstract-level, needs full-text verification".
>
> Labels: MEASURED, THEORY/MODEL, VENDOR CLAIM, OPINION (incl. practitioner), OWN CALCULATION.

## 1. Theory: why a hold-type display blurs tracked motion (box blur, sinc MTF); MPRT / BET; LCD vs. OLED

### Takeaway
On a sample-and-hold display each frame is static for the hold time while a pursuing eye keeps moving, so the image is smeared on the retina over a distance = speed × hold time; this is the dominant blur source (about 70% on 2005-era LCDs), independent of pixel response, and therefore applies equally to fast-response OLED. The box-blur/sinc form is the standard consequence of that mechanism, but I could confirm only the mechanism and the "width ∝ speed × frame time" scaling in sources, not the sinc equations in the named primary papers.

### Cited Findings
- **THEORY/MODEL [SNIPPET]** Hold-type displays present each frame constantly for (frame rate)^-1 seconds (the "hold time"); the continuously tracking eye smears it on the retina. Two signal-processing models of LCD blur exist (frequency domain and signal domain), both combining the display's temporal impulse response with human motion tracking and spatio-temporal low-pass filtering. — S. H. Chan, T. Q. Nguyen, "LCD Motion Blur: Modeling, Analysis, and Algorithm", IEEE Trans. Image Processing, 2011 — [ResearchGate record](https://www.researchgate.net/publication/49809087_LCD_Motion_Blur_Modeling_Analysis_and_Algorithm), [PubMed](https://www.pubmed.gov/21292596/), [author PDF (not reachable)](https://engineering.purdue.edu/ChanGroup/publication/J2011_Chan_Nguyen.pdf). The snippet states that its model generalises the "fundamental LCD motion blur equation" of Pan et al.
- **THEORY/MODEL [SNIPPET]** Pan, Feng & Daly build a general LCD motion-blur model on sampling-and-reconstruction theory covering both the LCD and the human visual system. — H. Pan, X.-F. Feng, S. Daly, "LCD motion blur modeling and analysis", IEEE ICIP 2005 — [IEEE Xplore 1529981](https://ieeexplore.ieee.org/document/1529981/keywords), [ResearchGate](https://www.researchgate.net/publication/4186310_LCD_motion_blur_modeling_and_analysis)
- **THEORY/MODEL + MEASURED [SNIPPET]** "The slow response of current LCD only contributes to 30% of the motion blur, while the hold-type rendering mode of LCD contributes to 70%"; slow response "is not a dominant factor"; black data insertion and frame-rate doubling each give roughly 50% blur reduction with fast LCDs. — H. Pan, X.-F. Feng, S. Daly, "51.4: Quantitative Analysis of LCD Motion Blur and Performance of Existing Approaches", SID Symposium Digest 36, pp. 1590–1593, 2005 — [ResearchGate](https://www.researchgate.net/publication/250999804_514_Quantitative_Analysis_of_LCD_Motion_Blur_and_Performance_of_Existing_Approaches), [Bohrium summary](https://www.bohrium.com/paper-details/51-4-quantitative-analysis-of-lcd-motion-blur-and-performance-of-existing-approaches/812132111644360706-15840)
- **THEORY/MODEL [SNIPPET, student project, secondary]** "The blur width is proportional to the motion speed and frame length"; for an ideal zero-order hold the 10–90% blurred edge width is 0.8·v·T (example: about 15 px), "entirely due to hold-type rendering". — L. Verslegers, "LCD Motion Blur", Stanford Psych 221 course project, 2010 — [page](http://acorn.stanford.edu/psych221/projects/2010/LievenVerslegers/LCD_Motion_Blur_Lieven_Verslegers.htm). (Consistent with a box of width v·T: the 10–90% span of a linear ramp is 0.8 of its length.)
- **DEFINITION [SNIPPET]** BET (Blurred Edge Time) is "usually measured on the METP [moving-edge temporal profile] as the time interval from 10% to 90% of the METP luminance range". Normalisation: N-BEW (frames) = BEW (px) / speed (px/frame); N-BET (s) = N-BEW × T (s/frame); MPRT (s) = average of N-BET over grey-level transitions. So speed in **pixels per frame** is exactly the factor converting a blur width in pixels to a time. — search summary drawing on A. B. Watson, "Display motion blur: Comparison of measurement methods", J. SID 18(2):179, 2010, DOI 10.1889/jsid18.2.179 — [NASA PDF (not reachable)](https://hsi.arc.nasa.gov/publications/watson-2010-jsid.pdf), [ResearchGate](https://www.researchgate.net/publication/245413174_Display_motion_blur_Comparison_of_measurement_methods); and on Tourancheau et al. — [CORE PDF](https://files01.core.ac.uk/download/pdf/53019082.pdf)
- **MEASURED [SNIPPET]** Watson 2010: under ICDM, eight flat panels (LCDs and PDPs, with various blur countermeasures) were measured by six instruments (pursuit camera, stationary high-speed camera, fixed non-imaging detector); good agreement between methods; new metric GET (Gaussian Edge Time, cumulative-Gaussian fit to the edge) correlates 0.97 with BET and is more robust; GET/BET is "largely proportional to hold time". — same Watson 2010 references; [HAL record of the BET/GET comparison](https://hal.archives-ouvertes.fr/hal-00486938)
- **MEASURED/GUIDELINE [SNIPPET]** Proposed acceptability guideline from an LCD blur annoyance study: acceptable BET < 20 ms, good < 11 ms, excellent < 6.6 ms. — Tourancheau et al., "61.3: Visual Annoyance and User Acceptance of LCD Motion-Blur", SID Digest — [ResearchGate](https://www.researchgate.net/publication/29608339_613_Visual_Annoyance_and_User_Acceptance_of_LCD_Motion-Blur). (Note: a 60 Hz full-persistence ideal hold display has BET ≈ 0.8 × 16.7 = 13.3 ms, i.e. only "acceptable" — OWN CALCULATION from the 0.8·T relation above.)
- **OPINION / PRACTITIONER [SNIPPET]** "Blur Busters Law": "1ms persistence = 1 pixel motion blur per 1000 pixels/second motion"; on sample-and-hold, 60 fps at 1000 px/s gives ~16.7 px blur, 1000 fps ~1 px; assumes square-wave (near-zero GtG) transitions, slow response adds on top; many OLEDs have sub-1 ms GtG yet 16.7 ms MPRT at 60 Hz; frame persistence applies to LCD and standard AMOLED alike; derived by the author from repeated tests of LCD monitors. — M. Rejhon, "Blur Busters Law: The Amazing Journey To Future 1000Hz Displays" — [article](https://www.blurbusters.com/blur-busters-law-amazing-journey-to-future-1000hz-displays-with-blurfree-sample-and-hold/), [forum post](https://forums.blurbusters.com/viewtopic.php?p=1570)

### Inferences
- THEORY (standard LTI result, **not confirmed verbatim in a named paper this session**): for perfect pursuit at speed v (px/frame) on a display that emits light for a fraction d (duty cycle) of the frame with a square temporal response, the retinal image is the frame convolved with a 1-D box of width w = v·d pixels along the motion direction; its MTF is |sinc(w·f)| = |sin(π w f)/(π w f)|, f in cycles/px. An LCD's finite response convolves a further (asymmetric) kernel on top, so real LCD MPRT > hold time; a fast OLED approximates the pure box.
- "Blur Busters Law" is the same statement in ms and px/s: w [px] = speed [px/s] × persistence [s]. It is not an independent finding.
- Blur is purely along the motion direction; detail perpendicular to the motion keeps full contrast.

### Gaps
- Could not open Klompenhouwer & Velthoven 2004, Klompenhouwer 2006, Kurita 2001, Feng 2006: the "temporal impulse response → spatial aperture → sinc" derivation and Kurita's original hold-type argument are therefore uncited here.
- Watson 2010 numerical BET/GET per display, and any just-noticeable-blur figure, not retrieved.
- No OLED-specific MPRT measurement found in a peer-reviewed source; only the practitioner statement above.

## 2. Quantitative: at which px/frame does the sinc MTF remove Nyquist and typical texture detail, and when does it cancel an unsharp mask?

### Takeaway
(All OWN CALCULATION.) With 100% persistence and a tracking eye, the blur width equals the motion in px per **displayed content frame**; detail of period P px drops to 50% contrast at 0.60·P px/frame and to zero at P px/frame. So 2-px (Nyquist) detail is half gone at 1.2 px/frame and nulled at 2 px/frame; an unsharp-mask boost of +10/+30/+60% at a given period is fully cancelled at about 0.24/0.39/0.51 × P px/frame — for Nyquist detail that is roughly 0.5–1 px/frame.

### Cited Findings
- The only sourced inputs are the scaling law "blur width ∝ speed × frame length" and the 0.8·v·T edge width — [Stanford project](http://acorn.stanford.edu/psych221/projects/2010/LievenVerslegers/LCD_Motion_Blur_Lieven_Verslegers.htm); the px/s × persistence form — [Blur Busters](https://www.blurbusters.com/blur-busters-law-amazing-journey-to-future-1000hz-displays-with-blurfree-sample-and-hold/). Everything below is derived from the box model.

### Inferences (OWN CALCULATION; assumptions: perfect smooth pursuit, square temporal response, duty d, blur width w = v·d px, MTF = |sinc(w/P)|)

Ratio x = w/P needed for a given MTF value (solved numerically):

| MTF | 0.909 (cancels +10%) | 0.90 | 0.833 (+20%) | 0.769 (+30%) | 0.714 (+40%) | 0.625 (+60%) | 0.50 | 0.20 | 0 |
|---|---|---|---|---|---|---|---|---|---|
| w/P | 0.24 | 0.25 | 0.33 | 0.39 | 0.44 | 0.51 | 0.60 | 0.83 | 1.00 |

MTF by blur width w (px) and detail period P (px):

| w (px) → | 0.5 | 1 | 2 | 3 | 4 | 6 | 8 |
|---|---|---|---|---|---|---|---|
| P = 2 (Nyquist) | 0.90 | 0.64 | 0 | 0.21* | 0 | 0 | 0 |
| P = 3 | 0.96 | 0.83 | 0.41 | 0 | 0.21* | 0 | 0.10* |
| P = 4 | 0.97 | 0.90 | 0.64 | 0.30 | 0 | 0.21* | 0 |
| P = 6 | 0.99 | 0.96 | 0.83 | 0.64 | 0.41 | 0 | 0.21* |
| P = 8 | 0.99 | 0.97 | 0.90 | 0.78 | 0.64 | 0.30 | 0 |

\* beyond the first zero the sinc lobe is negative: contrast-reversed (spurious resolution), not usable sharpness.

Thresholds in px per content frame (100% persistence, d = 1):

| Detail period | MTF 90% | MTF 50% | first zero | +10% USM cancelled | +30% | +60% |
|---|---|---|---|---|---|---|
| 2 px | 0.5 | 1.2 | 2 | 0.48 | 0.78 | 1.0 |
| 4 px | 1.0 | 2.4 | 4 | 0.95 | 1.6 | 2.0 |
| 8 px | 2.0 | 4.8 | 8 | 1.9 | 3.1 | 4.1 |

Same thresholds as screen speed in px/s (px/frame × fps), full persistence, fps = refresh:

| Rate | 2-px detail: 50% / zero | 4-px: 50% / zero | 8-px: 50% / zero |
|---|---|---|---|
| 30 fps | 36 / 60 px/s | 72 / 120 | 145 / 240 |
| 60 | 72 / 120 | 145 / 240 | 290 / 480 |
| 120 | 145 / 240 | 290 / 480 | 579 / 960 |
| 144 | 174 / 288 | 348 / 576 | 695 / 1152 |
| 240 | 290 / 480 | 579 / 960 | 1158 / 1920 |

- **In px-per-frame units the thresholds are independent of refresh rate**; a higher frame rate helps only because the same px/s becomes fewer px/frame.
- "Cancelled" means the net MTF (USM gain × sinc) is back to the unsharpened static value at that frequency. The unsharpened texture is attenuated by the same sinc, so the *ratio* sharpened/unsharpened is preserved in principle; what vanishes is absolute contrast. Whether the difference stays visible depends on the remaining contrast vs. the contrast threshold at that frequency, which the box model does not answer. Practical reading: once the sinc is below ~0.5 at the band the USM boosts (> 0.6·P px/frame), a +10…60% boost changes absolute contrast by only +5…30% of the static value, and at ≥ P px/frame the band is gone regardless.
- With duty cycle d < 1 (strobing/BFI) all px/frame thresholds scale by 1/d (e.g. 25% persistence: 2-px detail survives to 8 px/frame).
- Frame rate below refresh on a hold display (30 fps on 60 Hz): each content frame is held for two refreshes = 33.3 ms, so use px per *content* frame; equivalent to a 30 Hz hold display.
- The "+10–60% high-frequency boost" range for unsharp masking is the brief's assumption; I did not source it here (covered by other researchers).

### Gaps
- No source found that states these specific px/frame thresholds for texture sharpening; they are derivations.
- Real LCD response adds blur (Pan et al.: ~30% of the total on 2005 panels), so real thresholds are somewhat lower than the ideal-hold figures; no current-panel quantification retrieved.

## 3. Measured perception studies: blur/sharpness vs. speed and frame rate

### Takeaway
Psychophysical studies agree that motion quality rises steeply with frame rate and saturates around 240–250 fps, and that blur and judder both matter (judder more than hold blur in the Denes model); but I found no study, at snippet level, that directly measures "visibility of texture sharpening vs. px/frame".

### Cited Findings
- **MEASURED [SNIPPET]** 120 fps "provides good improvement compared to that of 60 fps"; improvement saturates at "about 240 fps for representative standard-resolution natural images"; 480 fps used as reference; around 250 fps is close to the perception limit for both blur and jerkiness. — Y. Kuroki, T. Nishi, S. Kobayashi, H. Oyaizu, S. Yoshimura, "A psychophysical study of improvements in motion-image quality by using high frame rates", J. SID 15(1), 2007, DOI 10.1889/1.2451560 — [Wiley abstract](https://sid.onlinelibrary.wiley.com/doi/abs/10.1889/1.2451560), [Semantic Scholar](https://www.semanticscholar.org/paper/A-psychophysical-study-of-improvements-in-quality-Kuroki-Nishi/3a27b0d53986643366149904a59fddc4dede9def)
- **MODEL + MEASURED [SNIPPET]** Denes et al. model quality loss from object motion, refresh rate and resolution as blur + judder; blur is split into hold-type display blur, eye-motion blur (imperfect smooth pursuit) and resolution-reduction blur; judder from the spatio-temporal spectrum; validated 50–165 Hz; judder-only model fits poorly (RMSE > 0.51), best fit needs judder + blur + motion predictability; measurements suggest at least ~250 Hz is needed to completely remove blur and judder; application: trade resolution against refresh rate depending on motion speed on a G-Sync monitor. Later work cites it as showing judder affects quality "far more than hold-type blur". — G. Denes, A. Jindal, A. Mikhailiuk, R. K. Mantiuk, "A perceptual model of motion quality for rendering with adaptive refresh-rate and resolution", ACM TOG 39(4), art. 133, 2020, DOI 10.1145/3386569.3392411 — [project page](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/), [Cambridge repository](https://www.repository.cam.ac.uk/handle/1810/305189), [PDF (not reachable)](https://www.cl.cam.ac.uk/~gd355/publications/sg20_paper.pdf)
- **MODEL [SNIPPET]** Follow-up that adds texture content: Jindal, Wolski, Myszkowski, Mantiuk, "Perceptual Model for Adaptive Local Shading and Refresh Rate", 2021, "extends the visual quality model to account for the important effect of texture content"; CaMoJAB predicts quality from refresh rate, shading resolution, persistence and velocity. — [paper PDF link](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf), [Jindal tech report UCAM-CL-TR-977](https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-977.pdf). This is the closest existing model to the user's question (it is built to decide when shading/texture resolution can be reduced under motion).
- **MEASURED [SNIPPET]** "It is acceptable to sample at frame rates 50% lower than the critical frame rate (CFR)", CFR being the rate at which motion artifacts (motion blur, temporal aliasing) become perceptible. — A. Mackin, K. C. Noland, D. R. Bull, "High Frame Rates and the Visibility of Motion Artifacts", SMPTE Motion Imaging Journal 126(5):41–51, 2017 — [SMPTE](https://journal.smpte.org/periodicals/SMPTE%20Motion%20Imaging%20Journal/126/5/15/), [Bristol record](https://research-information.bris.ac.uk/en/publications/high-frame-rates-and-the-visibility-of-motion-artifacts/)
- **THEORY/MODEL [SNIPPET]** Treating capture as temporal sampling and adding an eye-tracking model gives "minimum frame rate requirements that are many times higher than those in use today". — K. C. Noland, "The Application of Sampling Theory to Television Frame Rate Requirements", BBC R&D White Paper WHP 282, 2014 — [mirror of the BBC publication page (unofficial domain)](https://bbcbreakingnews.pages.dev/rd/publications/whitepaper282); related: Noland, "High Frame Rate Television: Sampling Theory, the Human Visual System, and Why the Nyquist–Shannon Theorem Does Not Apply", SMPTE MIJ 125(3), 2016 — [SMPTE](https://journal.smpte.org/periodicals/SMPTE%20Motion%20Imaging%20Journal/125/3/19/). A 700 Hz figure attributed to Noland appeared in one snippet without usable context.
- **THEORY/MODEL [SNIPPET]** Window of visibility = simplified human spatio-temporal contrast sensitivity function, used to compute "the lowest artifact-free frame rate"; critical frame rate rises linearly with motion speed; window size/shape depends on luminance, eccentricity, colour and eye movements; the model's parameters include speed (deg/s), frame rate, shutter, blur, hold. — A. B. Watson, "High Frame Rates and Human Vision: A View through the Window of Visibility", SMPTE Motion Imaging Journal 122(2), 2013, DOI 10.5594/j18266XY — [SMPTE](https://journal.smpte.org/periodicals/SMPTE%20Motion%20Imaging%20Journal/122/2/8/), [NASA NTRS record](https://ntrs.nasa.gov/citations/20130013545); original: Watson, Ahumada, Farrell, "Window of visibility", JOSA A, 1986 — [NTRS record](https://ntrs.nasa.gov/citations/19860047713)

### Inferences
- The saturation near 240–250 fps (Kuroki; Denes) implies that at 30–60 fps on a full-persistence monitor, hold blur and judder are both well above threshold for typical natural-image motion; the display is the limiting element for moving fine detail.
- Mackin et al.'s "50% below CFR is acceptable" shows *acceptability* is more lenient than *visibility*; analogous caution applies to sharpening: "visible in an A/B test" and "matters in use" are different thresholds.

### Gaps
- **No solid study found** that measures the visibility of sharpening / high-frequency texture boost as a function of screen speed. Denes 2020 / Jindal 2021 are the nearest models; their numerical resolution-vs-velocity results were not retrievable (PDFs blocked).
- Chapiro et al. 2019 (judder model) not retrieved at all.
- Specific deg/s velocities, CFR values and JOD numbers from these papers not retrieved.

## 4. Non-tracking case (fixation while the texture flows) and frame rate below refresh rate

### Takeaway
If the eye does not pursue, there is no hold-type smear; instead the image slips over the retina (acuity degrades above roughly 2.5–3°/s retinal speed) and low frame rates show judder/strobing rather than blur. Ground texture in a flight sim is a mix: the pilot mostly fixates instruments/horizon (non-tracking, peripheral, retinal slip) and occasionally tracks a ground feature (hold blur applies); in both regimes fast-moving fine detail is degraded. Controlled evidence for the 30-fps-on-60-Hz case is absent.

### Cited Findings
- **MEASURED [SNIPPET, secondary citations of Westheimer & McKee 1975]** Visual acuity declines when retinal image velocity exceeds about 2.5°/s (other citing papers say 3°/s). — cited in "Saccadic and smooth-pursuit eye movements during reading of drifting texts", Journal of Vision — [JoV](https://jov.arvojournals.org/article.aspx?articleid=2121460); original: Westheimer & McKee, "Visual acuity in the presence of retinal-image motion" — [ResearchGate record](https://www.researchgate.net/publication/22013675_Visual_acuity_in_the_presence_of_retinal-image_motion)
- **MEASURED [SNIPPET]** With pursuit gain 0.9–1.1 retinal text speeds stayed about 0.09–0.92°/s, below the 2.5°/s critical value. — same [JoV article](https://jov.arvojournals.org/article.aspx?articleid=2121460)
- **MEASURED [SNIPPET]** For Landolt optotypes moving up to 14°/s, dynamic-acuity thresholds of healthy controls were indistinguishable from stationary fixation; patients with reduced pursuit gain showed dramatically worse thresholds. — "Impaired analysis of moving objects due to deficient smooth pursuit eye movements", Brain 122(8):1495 — [Oxford Academic](https://academic.oup.com/brain/article/122/8/1495/504360?login=true). (Real moving targets, not a sampled display: shows the eye itself is not the limit at these speeds — the hold display is.)
- **THEORY/MODEL [SNIPPET]** When the eye tracks one object, untracked content has high retinal speed/falls in the periphery; smooth pursuit on high-persistence (hold-type) displays leads to increased perceived blur and edge flicker. — context of Watson 2013 and Mackin et al. 2017 — [SMPTE Watson](https://journal.smpte.org/periodicals/SMPTE%20Motion%20Imaging%20Journal/122/2/8/), [Bristol PDF link](https://research-information.bris.ac.uk/ws/files/184363894/Dave_Bull_High_Frame_Rates_and_the_Visibility_of_Motion_Artifacts.pdf)
- **OPINION / PRACTITIONER [SNIPPET]** 30 fps on a 60 Hz *impulse/strobed* display (CRT, strobed backlight) gives a double image (number of duplicate images = refresh rate / frame rate); on a sample-and-hold LCD there is no double image but more tracking blur (frame held 33 ms). A dissenting forum poster disputes the double image. — Blur Busters forums — [thread](https://forums.blurbusters.com/viewtopic.php?t=342), [AVS forum thread](https://www.avsforum.com/threads/30fps-gaming-at-60hz-and-the-effects-of-frame-doubling.1107325/)

### Inferences
- OWN CALCULATION: for a 27" 2560×1440 monitor at 60 cm (~45 px/deg, my assumption), 2.5°/s ≈ 112 px/s ≈ 1.9 px/frame at 60 fps or 3.7 px/frame at 30 fps. Above that, un-tracked texture is blurred by the retina itself, irrespective of the display.
- Regime for flight-sim ground texture (OPINION, mine): large-field optic flow with no single target; gaze is usually on the horizon/runway/instruments, so near-field ground flows un-tracked in the lower periphery, where acuity is low anyway. When the user deliberately inspects the ground (the situation in which sharpening is judged) the eye pursues, and section 2 applies. Optokinetic following of wide-field flow makes partial tracking likely even without intent, so the hold-blur estimate is the relevant upper bound on visible sharpness.
- With VRR or fps = refresh, the hold model is clean; with uneven frame pacing (30–45 fps on fixed 60 Hz, vsync) frames are held 1–3 refreshes irregularly, adding judder on top of the longest-hold blur.

### Gaps
- **No controlled perceptual study found** for 30 fps content on 60 Hz hold-type displays (blur vs. double image); only forum-level sources, which conflict.
- No source retrieved on gaze behaviour of sim pilots or on optokinetic tracking of ground flow.
- Westheimer & McKee value known only through secondary citations that disagree (2.5 vs 3°/s).

## 5. Typical screen velocities of ground texture in a flight sim (OWN CALCULATION)

### Takeaway
(All OWN CALCULATION.) Along-track ground motion expressed in *texels of the 2.4 m texture* per frame is simply v/(2.4 m × fps), independent of altitude and screen position: 0.4–1.7 texels/frame at 60 fps and 0.8–3.5 at 30 fps for 60–250 m/s. Hence, for a tracking eye on a full-persistence display, the finest texture detail (2-texel period, 4.8 m) is half lost at ~87 m/s (30 fps) / ~174 m/s (60 fps) and nulled at 144 / 288 m/s; and above roughly 1000–1500 m AGL the top mip level is minified below 1 px/texel and is not displayed at all.

### Cited Findings
- No external source; geometry only. Input from the brief: texture ≈ 2.4 m/px, 60–250 m/s, 300–3000 m, 60–90° HFOV, 30–60 fps.

### Inferences (OWN CALCULATION)
Assumptions: level flight, flat ground, camera pitch 0 (horizon at screen centre), pinhole projection, 2560×1440, HFOV 75° → focal length f = 1280/tan 37.5° = 1668 px; bottom edge is 23.3° below the horizon. For 4K multiply px values by 1.5; for HFOV 60°/90° f = 2217/1280 px.

- Ground point straight ahead at y px below the horizon: screen speed = v·y²/(f·h) px/s (v speed, h height AGL). Speed falls quadratically toward the horizon (→ 0 at the horizon). Looking straight down: f·v/h px/s.
- Texel footprint at y: lateral f·s·tanθ/h = s·y/h px per texel; along-track s·y²/(f·h) px per texel (s = 2.4 m). Dividing: **along-track motion = v/(s·fps) texels per frame**, everywhere on screen.

| v (m/s) / h (m) | bottom edge px/s | px/frame @60 / @30 | y = 240 px (1/3 down) px/s | straight-down view px/s | px per texel at bottom (lateral / along) |
|---|---|---|---|---|---|
| 60 / 300 (GA, low) | 62 | 1.0 / 2.1 | 6.9 | 334 | 5.8 / 2.5 |
| 100 / 300 | 104 | 1.7 / 3.5 | 11.5 | 556 | 5.8 / 2.5 |
| 60 / 1000 | 19 | 0.3 / 0.6 | 2.1 | 100 | 1.7 / 0.75 |
| 130 / 1500 | 27 | 0.45 / 0.9 | 3.0 | 145 | 1.15 / 0.5 |
| 250 / 300 (jet, very low) | 259 | 4.3 / 8.6 | 29 | 1390 | 5.8 / 2.5 |
| 250 / 3000 | 26 | 0.43 / 0.86 | 2.9 | 139 | 0.58 / 0.25 |
| 250 / 10000 (cruise) | 7.8 | 0.13 / 0.26 | 0.9 | 42 | 0.17 / 0.07 |

| speed | texels/frame @30 fps | @60 | @120 | @144 |
|---|---|---|---|---|
| 60 m/s | 0.83 | 0.42 | 0.21 | 0.17 |
| 100 m/s | 1.39 | 0.69 | 0.35 | 0.29 |
| 130 m/s | 1.81 | 0.90 | 0.45 | 0.38 |
| 250 m/s | 3.47 | 1.74 | 0.87 | 0.72 |

Combining with section 2 (tracking eye, 100% persistence; detail period in texels of the top mip level; blur width = texels/frame):
- 2-texel detail (the band an unsharp mask of radius ≈ 1 texel boosts most): MTF 50% at 1.2 texels/frame → v ≈ 87 m/s at 30 fps, 174 m/s at 60 fps, 348 m/s at 120 fps; zero at 2 texels/frame → 144 / 288 / 576 m/s.
- A +30% boost at that band is fully cancelled at 0.78 texels/frame → 56 m/s (30 fps), 112 m/s (60 fps); +60% at 1.0 texel/frame → 73 / 146 m/s; +10% at 0.48 texel/frame → 34 / 69 m/s.
- 4-texel detail: 50% at 174 m/s (30 fps) / 348 m/s (60 fps).
- Only along-track detail is affected; cross-track edges (e.g. roads parallel to the flight path) keep full contrast in the tracking model.
- Visibility precondition: the top mip level is shown only where the texture is ≥ ~1 px/texel (lateral, with anisotropic filtering). At the bottom edge that requires h ≲ s·720/1 ≈ 1700 m in this setup (≈ 2600 m at 4K); higher on the screen it requires proportionally lower altitude. So the top level is on screen mainly at low altitude and in the lower part of the image — exactly where screen motion is fastest.
- Angular speeds: at ~45 px/deg, 62–259 px/s ≈ 1.4–5.8°/s at the bottom edge for the low-altitude cases; well inside the range where pursuit kept acuity intact in the DVA study cited in section 4 (≤ 14°/s), and around/above the 2.5°/s retinal-slip limit when not tracked.

### Gaps
- Camera pitch in typical cockpit views (often nose-down, more ground visible), terrain relief, turns and actual X-Plane LOD/mip-bias behaviour are not modelled.
- The actual imagery scale depends on zoom level and latitude; 2.4 m is the brief's figure.

## 6. Counter-evidence: low-persistence displays and cases where sharpness stays visible in motion

### Takeaway
Hold blur is a property of persistence, not of motion as such: with strobed backlights/BFI/VR-style low persistence (or very high refresh), the box shrinks by the duty cycle and moving detail remains resolvable, and the human eye itself resolves fine detail on tracked targets up to at least ~14°/s. So "sharpening is invisible in motion" is true only for full-persistence displays at low frame rates and fast along-track motion — not at cruise altitude, slow flight, or on strobed displays.

### Cited Findings
- **MEASURED [SNIPPET]** Black data insertion / frame-rate doubling reduce motion blur by roughly 50% on fast LCDs; BDI shortens the display temporal aperture. — Pan, Feng, Daly, SID 2005 — [ResearchGate](https://www.researchgate.net/publication/250999804_514_Quantitative_Analysis_of_LCD_Motion_Blur_and_Performance_of_Existing_Approaches)
- **MEASURED [SNIPPET]** On a 240 Hz temporally multiplexed stereoscopic OLED, short duty cycles and low flash numbers reduce visibility of motion artifacts (judder, motion blur, edge banding), while long duty cycles and high flash numbers reduce flicker. — study surfaced under the Mackin et al. search (exact bibliographic identity not confirmed; treat as unverified) — [ResearchGate listing](https://www.researchgate.net/profile/Alex-Mackin/publication/318219284_High_Frame_Rates_and_the_Visibility_of_Motion_Artifacts/links/605a6a55299bf17367644275/High-Frame-Rates-and-the-Visibility-of-Motion-Artifacts.pdf)
- **OPINION / PRACTITIONER [SNIPPET]** On strobed/impulse displays persistence equals flash length: a 1 ms flash gives ~1 px blur at 1000 px/s. — M. Rejhon — [Blur Busters](https://www.blurbusters.com/blur-busters-law-amazing-journey-to-future-1000hz-displays-with-blurfree-sample-and-hold/)
- **MEASURED [SNIPPET]** Tracked real targets: acuity unchanged up to 14°/s in healthy observers. — [Brain 122(8):1495](https://academic.oup.com/brain/article/122/8/1495/504360?login=true)
- **MEASURED [SNIPPET]** Quality keeps improving up to ~240 fps (Kuroki 2007; Denes 2020: ≥ 250 Hz to remove blur and judder), i.e. observers do see the extra moving detail that shorter hold times preserve. — [Kuroki abstract](https://sid.onlinelibrary.wiley.com/doi/abs/10.1889/1.2451560), [Denes project page](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/)
- **MODEL [SNIPPET]** Jindal et al. 2021 found texture content matters for whether reduced shading resolution is visible in motion (model extension "to account for the important effect of texture content"). — [paper link](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)

### Inferences
- OWN CALCULATION: on a 144 Hz monitor at 144 fps, 60 m/s is 0.17 texels/frame → MTF at the 2-texel band = sinc(0.087) ≈ 0.99; at 250 m/s, 0.72 texels/frame → sinc(0.36) ≈ 0.80. Sharpening remains fully effective there. Even at 60 fps and 60 m/s (0.42 texels/frame) the 2-texel band keeps sinc(0.21) ≈ 0.93. The hold-blur argument therefore only bites for fast aircraft and/or ~30 fps.
- Sharpening artefacts (halos, noise, aliasing/shimmer of over-sharpened minified texture) are *temporal* artefacts in motion and may be more, not less, visible than in stills when the eye does not track (strobing/judder of high-contrast edges). This is an inference; no source retrieved.
- A slight sinc pre-compensation argument cuts the other way: motion-compensated inverse filtering (the subject of Klompenhouwer & Velthoven 2004, per its title) deliberately boosts high frequencies along the motion direction to offset hold blur, so a static unsharp mask partly compensates hold blur at moderate speeds (below the first sinc zero). Not verified in the paper.

### Gaps
- No peer-reviewed VR low-persistence source retrieved.
- No study found on visibility of sharpening halos/shimmer under motion.
- Klompenhouwer & Velthoven 2004 content not verified beyond its title.
