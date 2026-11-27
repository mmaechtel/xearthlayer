# Round 2: verification at source and counter-evidence — sharpness perception in motion, sample-and-hold blur

> **Access status (read first).** The `WebFetch` tool failed with DNS errors for every host, including the newly allowed ones. Direct downloads (`curl` through the session proxy, then `pdftotext`) worked for **arxiv.org, www.cl.cam.ac.uk, ntrs.nasa.gov, en.wikipedia.org, raw.githubusercontent.com**, and for **one** page on pmc.ncbi.nlm.nih.gov.
> **Not reachable in practice:** `pubmed.ncbi.nlm.nih.gov` (answers every request with a browser proof-of-work/cookie check), `pmc.ncbi.nlm.nih.gov` after the first page (reCAPTCHA "checking your browser"), `apps.dtic.mil` (returns an HTML stub instead of the PDF), `blurbusters.com` (403), `netflixtechblog.com` (403, JavaScript check), `doi.org` (403), `github.com` HTML (403). I did not try to get around these bot checks.
> Consequence: the classic vision-science papers that live only on PubMed or publisher sites (Burr & Ross 1982, Kelly 1979, Ludvigh & Miller 1958, Westerink & Teunissen, Bex/Hammett/Georgeson, Suchow & Alvarez, Choi/Cormack/Bovik) and Pan/Feng/Daly, Kuroki 2007, Mackin/Noland/Bull could **not** be opened. Their first-round numbers stay at abstract/snippet level.
>
> Markers: **[OPENED]** = full text read this round; **[ABSTRACT OPENED]** = abstract page read this round. Labels: MEASURED / MODEL / OPINION / OWN CALCULATION. "Secondary" = statement about paper A read in the full text of paper B.

Sources opened this round (referred to by short name below):

| Short name | Source | Status |
|---|---|---|
| Denes 2020 | G. Denes, A. Jindal, A. Mikhailiuk, R. K. Mantiuk, "A perceptual model of motion quality for rendering with adaptive refresh-rate and resolution", ACM TOG 39(4), art. 133 (SIGGRAPH 2020) — [PDF](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf), [project page](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/) | [OPENED] |
| Jindal 2021 | A. Jindal, K. Wolski, K. Myszkowski, R. K. Mantiuk, "Perceptual Model for Adaptive Local Shading and Refresh Rate", ACM TOG (SIGGRAPH Asia 2021) — [PDF](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf) | [OPENED] |
| Yee | H. Yee, "Spatiotemporal sensitivity and visual attention for efficient rendering of dynamic environments" (thesis-length version), arXiv cs/0511032 — [PDF](https://arxiv.org/pdf/cs/0511032) | [OPENED] (sections 3.1.2–3.1.3) |
| WoV 1983 | A. B. Watson, A. J. Ahumada, J. E. Farrell, "The window of visibility: a psychophysical theory of fidelity in time-sampled visual motion displays", NASA Technical Paper, 1983 (report version of the 1986 JOSA A paper) — [PDF](https://ntrs.nasa.gov/api/citations/19830025267/downloads/19830025267.pdf) | [OPENED] (OCR text) |
| Watson slides | A. B. Watson, "Frame Rate and Human Vision", slide deck dated 24 Oct 2012, NTRS 20130013545 — [PDF](https://ntrs.nasa.gov/api/citations/20130013545/downloads/20130013545.pdf) | [OPENED] — **slides, not the SMPTE 2013 article** |
| Watson blur | A. B. Watson, "Comparison of motion blur measurement methods", SID 2009 submission dated 13 Nov 2008, NTRS 20110003569 — [PDF](https://ntrs.nasa.gov/api/citations/20110003569/downloads/20110003569.pdf) | [OPENED] — **conference precursor of the J. SID 2010 paper** |
| Burr & Morgan | D. C. Burr, M. J. Morgan, "Motion deblurring in human vision", Proc. R. Soc. B 264(1380):431–436, 1997, PMID 9107056 — [PMC1688277](https://pmc.ncbi.nlm.nih.gov/articles/PMC1688277/) | [ABSTRACT OPENED] |
| LIVE-YT-HFR | P. C. Madhusudana et al., "Subjective and Objective Quality Assessment of High Frame Rate Videos", IEEE Access 2021, arXiv 2007.11634 — [PDF](https://arxiv.org/pdf/2007.11634) | [OPENED] (results and conclusion) |
| Hacking VMAF | "Hacking VMAF and VMAF NEG: vulnerability to different preprocessing methods", arXiv 2107.04510v2, 2021 (author names not checked in the text) — [PDF](https://arxiv.org/pdf/2107.04510) | [OPENED] |
| RPO-AdaSharp | "Region-Adaptive Video Sharpening via Rate-Perception Optimization", arXiv 2508.08794, 2025 (author names not checked) — [PDF](https://arxiv.org/pdf/2508.08794) | [OPENED] (intro and user study) |
| Drone racing | "Human-Piloted Drone Racing: Visual Processing and Control", arXiv 2103.04672, 2021 — [PDF](https://arxiv.org/pdf/2103.04672) | [OPENED] (abstract, results on gaze) |
| NASA flow 1992 | "Optical flow versus retinal flow as sources of information for flight", NASA conference paper N92-21472, NTRS 19920012229 — [PDF](https://ntrs.nasa.gov/api/citations/19920012229/downloads/19920012229.pdf) | [OPENED], but the OCR is too broken to quote sentences |
| Wikipedia | "Smooth pursuit", "Display motion blur", "Optokinetic response" (wikitext) — [SP](https://en.wikipedia.org/wiki/Smooth_pursuit), [DMB](https://en.wikipedia.org/wiki/Display_motion_blur), [OKR](https://en.wikipedia.org/wiki/Optokinetic_response) | [OPENED]; tertiary, weak |

## 1. Which first-round numbers are confirmed at source, corrected, or still unverified?

### Takeaway
Confirmed in full text: the box-filter hold-blur model (width = velocity / refresh rate, attributed by Denes to Klompenhouwer 2004), the Daly constants 0.82 / 0.15 deg/s / 80 deg/s (via two secondary full texts), the window-of-visibility law (critical rate linear in speed) and Burr & Morgan's discrimination result. Three first-round statements need correcting (below). All the 1958–2002 psychophysics numbers except Burr & Morgan remain unverified, because PubMed and publisher hosts could not be read.

### Cited Findings

**Status table**

| First-round item | Status | Evidence this round |
|---|---|---|
| Daly eye-movement constants 0.82, 0.15 deg/s, 80 deg/s | **CONFIRMED (secondary, two full texts)** | Yee eq. 3.1; Denes 2020 section 2.3 |
| Daly acuity term `k_max = 45.9/(v+2)` | **STILL UNVERIFIED** | Not in Yee's text; Daly 1998 (SPIE) not reachable |
| Kelly 1979: curve shifts to lower spatial frequency with retinal velocity; 0.15 deg/s | **CONFIRMED (secondary)**; the "0.15 vs 2 deg/s conflict" is resolved | Yee section 3.1.2 |
| Hold blur = box filter of width v/f; scaling by duty cycle | **CONFIRMED at source, now attributable** | Denes 2020 eq. 4 and section 7.2; Jindal 2021 eq. 14 |
| Sinc spectrum of a held frame | **CONFIRMED at source for the non-tracking case** | WoV 1983, "staircase motion" section |
| Watson 2010: mechanism, BET = 10–90% time, eight displays, six instruments | **CONFIRMED** (precursor paper) | Watson blur |
| Watson 2010: GET correlates 0.97 with BET; "largely proportional to hold time" | **STILL UNVERIFIED** | Precursor text has the Gaussian fit but I did not find these two statements |
| Watson 2013: critical frame rate rises linearly with speed; hold and eye movements are model terms | **CONFIRMED** (slide deck only) | Watson slides; law itself in WoV 1983 |
| Denes 2020: blur = hold + eye-motion + resolution; 50–165 Hz | **CONFIRMED** | Denes 2020 |
| Denes 2020: "measurements suggest at least ~250 Hz" | **CORRECTED** — this is Denes citing Kuroki, not a Denes result | Denes 2020 section 3.1 |
| "Judder affects quality far more than hold-type blur" attributed to Denes | **CORRECTED** — Jindal attributes it to Mackin et al. 2016 | Jindal 2021 section 2 |
| Kuroki 2007: saturation around 240 fps | **CONFIRMED (secondary)** | Jindal 2021 section 2; Denes 2020 section 3.1 |
| Mackin/Noland/Bull "50% below critical frame rate acceptable" | **PARTLY CONFIRMED (secondary, fragment)** | Denes 2020 section 3.2 mentions "about 50% of the critical refresh rate" for Mackin 2016; primary not opened |
| Noland: 700 Hz figure | **CONFIRMED (secondary)** with context | Denes 2020 section 3.1 |
| Burr & Morgan 1997 conclusion and 40/150 ms | **CONFIRMED (abstract at source)** | PMC1688277 |
| Burr & Ross 1982 (peak sensitivity constant to 800 deg/s) | **STILL UNVERIFIED** | PubMed gated |
| Ludvigh & Miller 1958 (1.95 / 2.84 / 6.63 arcmin; cubic law) | **STILL UNVERIFIED** | JOSA and DTIC not reachable |
| Westerink & Teunissen 1990/1995 (0.5–35 deg/s; fixation up to 5.3 deg/s) | **STILL UNVERIFIED** | Not on any reachable host |
| Bex, Edgar & Smith 1995; Hammett & Bex 1996; Hammett, Georgeson & Gorea 1998; Georgeson & Hammett 2002 ("up to 50%", 8–24 ms) | **STILL UNVERIFIED** | PubMed gated; PMC blocked after first page |
| Pan/Feng/Daly 70% hold / 30% response | **STILL UNVERIFIED** | Not on any reachable host |
| Blur Busters Law wording | **STILL UNVERIFIED** | blurbusters.com returned 403 |
| Westheimer & McKee 2.5–3 deg/s | **STILL UNVERIFIED** | Not opened |

**Decisive passages**

- MODEL (secondary on Daly 1998). "v_R = v_I − min(0.82 v_I + v_Min, v_Max)" with "v_Min is 0.15 deg/sec (the drift velocity of the eye), v_Max is 80 deg/sec (which is the maximum velocity that the eye can track efficiently). The value 0.82 accounts for Daly's data fitting that indicates the eye tracks all objects in the visual field with an efficiency of 82%." — [Yee, arXiv cs/0511032, eq. 3.1](https://arxiv.org/pdf/cs/0511032)
- MODEL (secondary on Kelly 1979). "Above the retinal velocity of 0.15 deg/sec, the peak sensitivity drops and the entire curve shifts to the left. This shift implies that waveforms of higher frequency become increasingly difficult to discern as the velocity increases." And: "the sensitivity function obtained at the retinal velocity of 0.15 deg/sec matched with the static CSF"; 0.15 deg/s is the drift of a fixating eye. So 0.15 deg/s is the baseline (static) condition, not a threshold where acuity starts to collapse. — [Yee](https://arxiv.org/pdf/cs/0511032)
- MODEL. "we follow the same practice as Klompenhouwer et al. [2004], and approximate hold-type blur with a box filter of the width (in visual degrees): b_D = v/f, where v is the object velocity in degrees per second and f is the refresh rate in Hz." — [Denes 2020, section 5.1.1](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MODEL. For low-persistence displays "we replace (v/f) with (vp/f) in Equation 4, where p is the fraction of the frame when the display is on"; the authors add that the model "was fitted and validated only on a high-persistence LCD display". — [Denes 2020, section 7.2](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MODEL. "Eye-motion blur (b_E), and hold-type blur (b_D) will blur the image only in the direction of motion". A box of width w is approximated by a Gaussian with σ = w/π. — [Denes 2020, section 5.2](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MEASURED. Eye tracking of five participants following a target at 12–36 deg/s: "SPEM is not affected by refresh rates above 27.5 Hz, but is affected by the nature of motion"; "saccades are more frequent during unpredictable motion than predictable motion (4.89 vs. 1.56 saccades per second)"; tracking delay "0.092 s vs. 0.004 s". Eye-blur model b_E = p_a·v + p_b with p_a = 0.001648, p_b = 0.079818 (predictable) and p_a = 0.004978, p_b = 0.176820 (unpredictable). "The common velocity gain of 0.82 [Daly 1998] would yield a linear gradient of p_a = 0.0045 under the 25ms integration window, which our data for unpredictable SPEM agrees with. However, for predictable motion, our results indicate much more accurate tracking (and hence less blur)." — [Denes 2020, section 6.1 and Table 1](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MEASURED. Quality experiment used velocities of 15, 30 and 45 deg/s at 50–165 Hz: "Quality increases with refresh rate, but the increase slows down above 100 Hz. Lower velocities are perceived to be of higher quality." Judder: "Observers were unable to tell the difference between juddery and non-juddery animations above 60 Hz"; "effect of judder on quality is negligible at 72 Hz and higher refresh rates". — [Denes 2020, Figs. 7 and 16, section 6.2](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- Secondary (correction). "Kuroki et al. [2006; 2007] arrived at a more conservative estimate using psychophysical measurements showing that at least 250 Hz is required to completely remove motion blur and judder." and Noland's model "predicts that while 140 Hz is sufficient for untracked motion, tracked motion requires at least 700 Hz for the illusion of perfect motion. The authors highlight that the figures should only be considered approximate, partly due to the limitations of Daly's model of SPEM". — [Denes 2020, section 3.1](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- Secondary (correction). "Mackin et al. [2016] report that even for a simple stimulus, it can take upto 600 Hz (depending upon the velocity) to achieve an artifact-free motion. However, observers found the motion artifacts beyond 300 Hz to be tolerable. They also noticed that temporal aliasing artifacts (judder and ghosting) contribute more to motion quality impairment compared to hold-type blur. ... Kuroki et al. [2007] measured motion quality up to refresh rate of 480 Hz and found that the quality improves rapidly with increasing refresh rate but saturates after 240 Hz for natural images." — [Jindal 2021, section 2](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)
- MODEL. Jindal 2021 uses the same box model with persistence p (eq. 14) and eye blur b_e = (c_a·v + c_b)·p with Denes's predictable coefficients (eq. 15), and replaces the sinc by a Lorentzian because "using a sinc(·) function introduces non-monotonicity in the model predictions". — [Jindal 2021, section 4](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)
- MODEL + MEASURED. Window of visibility: the "predicted critical sampling frequency is linear function of velocity, with an intercept given by the temporal frequency limit, and slope given by the spatial frequency limit" (OCR cleaned). Measured for two observers with a moving line: "For both observers the intercept is at about 30 Hz ... The slope ... is 6 cycles/deg for one observer and 13 cycles/deg for the other", which the authors call "somewhat low ... but not unreasonable given the low contrast". — [WoV 1983](https://ntrs.nasa.gov/api/citations/19830025267/downloads/19830025267.pdf)
- MODEL. Held frames ("staircase" presentation, where "each presentation lasts the full interval between steps") are derived by convolving the sampled motion with a unit pulse; the frequency spectrum of the stair function is "a sinc function", "falling to its first zero at w_s" (the sampling frequency). This is the hold/sinc model stated at source, for an eye that is **not** tracking (a temporal-frequency sinc). — [WoV 1983](https://ntrs.nasa.gov/api/citations/19830025267/downloads/19830025267.pdf)
- MODEL. Watson's slide formula treats display hold as a temporal convolution: `(((o(x) * m(x,t)) * e(t)) s(t)) * f(t) * h(t)`, i.e. a multiplicative `H(w)` in the frequency domain; critical frame rate `w_s = r·u_0 + w_0` (slope = spatial limit, intercept = temporal limit). Eye-movement slide: "When the eye tracks an object, its retinal motion is reduced or eliminated. Other objects are not tracked, but they lie in the low-resolution periphery." — [Watson slides](https://ntrs.nasa.gov/api/citations/20130013545/downloads/20130013545.pdf)
- THEORY + definition. "Motion blur arises when the eye tracks a moving image, while the display presents individual frames that persist for significant fractions of a frame duration, or longer. As a result, the image is smeared across the retina during the frame duration." The blurred-edge width "is usually proportional to the speed of motion"; BET is "the time interval (msec) between 10% and 90% points of the curve". A photodiode step response "can then be convolved with a pulse of one frame duration" to get the same blurred edge (attributed to Watson 2006). Experiment: eight displays (LCD and PDP), six instruments, 20 grey-to-grey transitions, edge speeds 8 and 16 pixels/frame. — [Watson blur](https://ntrs.nasa.gov/api/citations/20110003569/downloads/20110003569.pdf)
- MEASURED. "motion increased thresholds for blur discrimination, both for brief (40 ms) and for longer (150 ms) exposures. We conclude that motion deblurring is a subjective effect, which does not enhance visual discrimination performance. Moving objects appear sharp, not because of some special mechanism that removes blur, but because the visual system is unable to perform the discrimination necessary to decide whether the moving object is really sharp or not." Stimuli: moving Gaussian-blurred edges and bars. — [Burr & Morgan 1997, PMC1688277](https://pmc.ncbi.nlm.nih.gov/articles/PMC1688277/)

### Inferences
- OWN CALCULATION. Converting Denes's slopes back to a pursuit gain over the 25 ms window (slope / 0.025 s = slip fraction): predictable motion 0.001648 / 0.025 = 6.6% slip, i.e. gain about 0.93; unpredictable 0.004978 / 0.025 = 19.9% slip, gain about 0.80. Daly's 0.82 therefore describes unpredictable motion; for steady, predictable flow (ground streaming under an aircraft in straight flight) the first-round "18% residual slip" is about three times too pessimistic.
- The box model can now be cited as: Denes et al. 2020 eq. 4, "same practice as Klompenhouwer et al. [2004]"; Watson's "convolved with a pulse of one frame duration"; and Watson/Ahumada/Farrell for the sinc of a held frame. The first-round px/frame tables (all OWN CALCULATION) follow from this model and remain arithmetic, not measurements.
- Denes's constant term p_b = 0.08 deg (about 4.8 arcmin) is larger than normal static acuity would allow if read literally as retinal blur. It is a fitted parameter from eye-tracker traces (measured as the extreme-position difference inside 25 ms windows) and very likely includes tracker noise and fixational jitter. Treat it as a model coefficient, not as a physical blur width.

### Gaps
- Not opened: Burr & Ross 1982, Kelly 1979, Ludvigh & Miller 1958, Westerink & Teunissen 1990/1995, Bex/Edgar/Smith 1995, Hammett & Bex 1996, Hammett/Georgeson/Gorea 1998, Georgeson & Hammett 2002, Daly 1998, Pan/Feng/Daly 2005, Klompenhouwer & Velthoven 2004, Kuroki 2007, Mackin/Noland/Bull 2017, Watson 2013 (SMPTE article), Watson 2010 (J. SID article), Chapiro 2019, Blur Busters. To finish verification the session needs working access to pubmed.ncbi.nlm.nih.gov and pmc.ncbi.nlm.nih.gov (bot checks), plus publisher hosts (sciencedirect, opg.optica.org, spiedigitallibrary, sid.onlinelibrary.wiley.com, royalsocietypublishing.org).
- The two NTRS Watson documents are a slide deck and a conference precursor, not the journal articles the first round cited.

## 2. Counter-evidence: in which regimes does the literature predict sharpening stays visible?

### Takeaway
The opened sources contradict a blanket "sharpening is lost in motion". For predictable motion that the eye tracks, pursuit is accurate (gain about 0.93), and the two Cambridge models both state that resolution loss is most visible at low velocities and on high-frequency textures — which is exactly the flight-sim ground case at roughly 1–6 deg/s. Hold blur only caps detail at low frame rates or fast, low flight; the Denes experiments start at 15 deg/s, so the slow regime is model extrapolation, not measurement.

### Cited Findings

**2a. "Motion reduces high-frequency sensitivity, so sharpening benefit shrinks"**
- MODEL, calibrated on experiments (counter-evidence). The Jindal method "assigns higher shading rate to the regions where quality degradation due to variable-rate shading (VRS) is more visible (high-frequency textures, low velocities) and lower shading rates to less perceptible regions (smooth textures, high velocities)". Figure 2 caption: "The motion blur may mask the texture distortions". Loss of texture resolution is therefore modelled as visible at low velocity and masked at high velocity. — [Jindal 2021, Fig. 1 and Fig. 2](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)
- MODEL (counter-evidence). For large rendering budgets "the model recommends keeping the refresh rate and the resolution constant up to a certain velocity and then to gradually increase the refresh rate at the cost of the resolution". The model's premise, stated in the introduction, is that with slow movement spatial resolution should be maximised. — [Denes 2020, section 7 and Fig. 19](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MEASURED (counter-evidence). Predictable motion is tracked with far less slip than Daly's 82% model, with only 1.56 catch-up saccades per second. — [Denes 2020, section 6.1](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- MEASURED (supports first round). Motion raises blur-discrimination thresholds at 40 ms and 150 ms exposure. The abstract does not say whether observers tracked the stimuli, nor give speeds. — [Burr & Morgan 1997](https://pmc.ncbi.nlm.nih.gov/articles/PMC1688277/)
- MEASURED (counter-evidence, indirect). In a 480-video study, "Videos involving significant camera motion almost always received higher quality scores at high frame rates"; the gain "diminishes somewhat above 60 fps", and differences between 98 and 120 fps become "noticeable only when there is high motion". Viewers therefore do notice clarity differences in moving content. — [LIVE-YT-HFR, conclusion](https://arxiv.org/pdf/2007.11634)

**2b. "Hold blur caps visible texture detail"**
- MODEL (qualifies first round). Hold blur is one of three additive blur terms; it is scaled by the duty cycle p on low-persistence displays, and falls with 1/f. Denes notes motion artifacts on high- and low-persistence displays "are significantly different" and restricts the study to high-persistence LCDs. — [Denes 2020, sections 1.1, 5.1, 7.2](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- Secondary. "it is necessary to lower persistence to significantly reduce the hold-type blur inherent to LCDs as simply increasing the refresh rate is not effective [Sluyterman 2006]"; strobing backlights are common in HMDs and gaming monitors, but "psychophysical studies on the range of persistence used in these devices are quite limited". — [Jindal 2021, section 2](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)
- THEORY (qualifies first round). Hold blur requires tracking: it arises "when the eye tracks a moving image". When the eye does not track, the held frame gives a temporal sinc and the artifact is sampled-motion visibility (judder), governed by the window of visibility, not spatial smear. — [Watson blur](https://ntrs.nasa.gov/api/citations/20110003569/downloads/20110003569.pdf); [WoV 1983](https://ntrs.nasa.gov/api/citations/19830025267/downloads/19830025267.pdf)
- Tertiary. Catch-up saccades "are more common when pursuing at high speeds"; smooth pursuit "without a moving visual stimulus is difficult ... and typically results in a series of saccades"; pursuit cannot always be separated from "the slow phase of the optokinetic nystagmus". — [Wikipedia, Smooth pursuit](https://en.wikipedia.org/wiki/Smooth_pursuit)

**2c. Where do pilots look; is the fixated ground stable?**
- MEASURED (analogy, not flight-sim ground viewing). First-person-view drone racing pilots "fixated their eye gaze most of the time on the upcoming gates", with first fixations about 1.5 s and 16 m before passing the gate. Gaze goes to future waypoints, not to the fast near-field ground. — [Drone racing, arXiv 2103.04672](https://arxiv.org/pdf/2103.04672)
- Tertiary. "nearly any moving texture evokes OKR in mammals"; during the slow phase "the eyes smoothly follow the direction of the stimulus". So wide-field ground flow tends to pull the eye along, partly stabilising it without intent. — [Wikipedia, Optokinetic response](https://en.wikipedia.org/wiki/Optokinetic_response)
- OPINION/analysis (OCR-degraded; paraphrase only). A NASA paper on flight argues that only retinal flow, not optical flow, is relevant; that a pilot often cannot afford to look directly below at the fastest flow; and that a fixated object is followed with pursuit. — [NASA flow 1992](https://ntrs.nasa.gov/api/citations/19920012229/downloads/19920012229.pdf)

**2d. Halos, and do viewers prefer sharpened video?**
- MEASURED (against "sharpening is always preferred"; says nothing about motion). Crowdsourced comparison, 114 participants, 1,584 responses, six videos: "GT [original] proved to be better than the preprocessed video in 58.3% of cases"; "The combination of gamma correction and unsharp masking provided the worst visual quality." Participants compared "pairs of images", so this is a still-frame judgement. Unsharp masking raised VMAF, and the paper cites earlier work that unsharp masking "can artificially increase VMAF by 5–6%". — [Hacking VMAF, arXiv 2107.04510](https://arxiv.org/pdf/2107.04510)
- OPINION + small MEASURED. Uniform unsharp masking produces noise in flat regions and "overshoot artifacts" in densely textured regions, while "moderately textured regions produce visually improved results after sharpening". User study (n = 20, 10 videos): the region-adaptive method "consistently received higher preference" than plain unsharp masking, both followed by H.265 encoding. Motion speed was not a variable. — [RPO-AdaSharp, arXiv 2508.08794](https://arxiv.org/pdf/2508.08794)

**2e. Aliasing/shimmer vs. sharpness gain**
- MODEL + MEASURED. Sampled-motion artifacts become invisible once the frame rate exceeds `w_l + v·u_l` (temporal limit plus speed times spatial limit); measured intercept about 30 Hz, slopes 6 and 13 c/deg. — [WoV 1983](https://ntrs.nasa.gov/api/citations/19830025267/downloads/19830025267.pdf)
- MEASURED. Judder undetectable above 60 Hz and negligible for quality at 72 Hz and above, for 15–45 deg/s stimuli. — [Denes 2020, section 6.2](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- Secondary. "flicker can be detectable up to 500 Hz during saccadic eye motion [Davis et al. 2015]"; temporal aliasing "contribute[s] more to motion quality impairment compared to hold-type blur" (Mackin 2016). — [Jindal 2021, section 2](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)

### Inferences
- OWN CALCULATION (Denes model, 45 px/deg assumed as in round 1; ground-texture screen speeds of 1.4–5.8 deg/s from round 1, section 5). Hold-blur box width v/f:

  | Speed | 30 fps | 60 fps | 144 fps |
  |---|---|---|---|
  | 1.4 deg/s | 0.047 deg (2.1 px) | 0.023 deg (1.05 px) | 0.010 deg (0.44 px) |
  | 5.8 deg/s | 0.193 deg (8.7 px) | 0.097 deg (4.35 px) | 0.040 deg (1.8 px) |

  These equal round 1's px/frame figures, as they must. New information: in the Denes model the hold term is smaller than the (velocity-independent) eye-motion term for predictable motion below about 2.5 deg/s at 30 fps, 5.3 deg/s at 60 fps and 15 deg/s at 144 fps (solving v/f = 0.0798 + 0.001648·v). So in that calibrated model, hold blur is **not** the dominant blur at slow GA-type screen speeds on a 60 Hz or faster display. Caveat: the constant term is probably inflated by tracker noise (see section 1), so the crossover speeds are indicative only.
- Regime map (synthesis of the cited models; no source tested sharpening directly):

  | Regime | Prediction for a top-mip unsharp mask |
  |---|---|
  | Tracked, predictable, slow (below about 5 deg/s; under 1 px/frame), 60 fps or more | Stays visible. Pursuit gain about 0.93, hold box under about 1 px, models say resolution matters most here. |
  | Tracked, 30 fps, or fast low flight (above about 2 px/frame along track) | Along-track fine detail largely removed by the hold box; sharpening of the finest band mostly lost. Cross-track detail unaffected (blur is motion-parallel only). |
  | Tracked on strobed/BFI/VR display (duty p) | Box shrinks to v·p/f; sharpening stays visible to 1/p times higher speed. Model extrapolation; little psychophysics. |
  | Not tracked (gaze on horizon, instruments, waypoint), ground in periphery | No hold smear. Detail is limited by retinal velocity and peripheral acuity; discrimination of sharpness degraded (Burr & Morgan). Sharpening mostly not seen where the flow is fast; any contrast boost on strong edges adds to judder energy at 30 fps, negligible at 72 Hz and above. |
  | Far field near the horizon, slow on screen | Retinally near-static, but drawn from lower mip levels, so top-level sharpening contributes little (round 1 geometry). |

- OWN CALCULATION (window of visibility, using the measured 30 Hz and 13 c/deg from WoV 1983). Critical rate for an untracked pattern: 1.4 deg/s gives 30 + 1.4 × 13 ≈ 48 Hz; 5.8 deg/s gives about 105 Hz. So at 30 fps untracked ground detail is below the critical rate even at slow speeds (sampling artifacts potentially visible), at 60 fps only the faster near field is. These limits were measured at low contrast with a single line; higher-contrast content pushes them up.
- Halo inference from round 1 needs narrowing (own reasoning). In a linear model the overshoot **is** the high-frequency boost: for a small-radius unsharp mask (about 1 texel), halo and sharpening occupy the same band and are attenuated together by the hold box or retinal smear. "Halos survive motion better than the detail" can only hold for large radii, where the overshoot is spread over several texels and lies at lower spatial frequency than texel-scale detail. I found no measurement for or against either case.
- Sharpening preference: the one opened study that isolates unsharp masking found originals preferred slightly more often than processed versions on still frames; the adaptive-sharpening paper shows the dislike is driven by overshoot and noise in flat or very busy regions. Neither supports a claim that viewers generally prefer sharpened moving video, and neither tests it.
- Net direction: round 1's two headline conclusions are too strong as general statements and correct only in a bounded regime (30 fps and/or more than about 1–2 px/frame of along-track motion, or untracked viewing). For a GA aircraft at 60 fps or more with the viewer looking at the ground, the opened literature predicts the sharpening remains largely visible.

### Gaps
- Motion silencing (Suchow & Alvarez 2011; Choi, Cormack & Bovik 2015) could not be opened (PubMed gated; no arXiv copy found by one search). Whether fast motion hides shimmer therefore remains **unverified** this round.
- Westerink & Teunissen (the only direct measurement of perceived sharpness of moving natural images under fixation vs. pursuit) still unopened, so the "compression of sharpness differences" claim rests on abstracts.
- Netflix's own description of VMAF NEG and the "enhancement gain" rationale was not readable (blog 403; the VMAF FAQ on GitHub has no sharpening section).
- No pilot or flight-sim gaze study over ground texture was found; drone racing and Wikipedia's OKR description are analogies.
- NASA flow 1992 could only be paraphrased; author and exact wording not recoverable from the OCR.

## 3. What remains genuinely unstudied?

### Takeaway
No opened source measures visibility of sharpening, overshoot or halos as a function of image speed, and the best quantitative models were never tested below 15 deg/s — the range that matters for ground texture in a flight sim.

### Cited Findings
- The Denes quality experiment used 15, 30 and 45 deg/s (plus sinusoidal and unpredictable motion) at 50–165 Hz; its stimuli "did not contain spatial blur orthogonal to the direction of the motion", and the weight for orthogonal (resolution) blur was set by "an expert observer", not by an experiment. — [Denes 2020, sections 4 and 6](https://www.cl.cam.ac.uk/research/rainbow/projects/motion_quality_model/denes20_comp.pdf)
- Persistence: "psychophysical studies on the range of persistence used in these devices are quite limited". — [Jindal 2021, section 2](https://www.cl.cam.ac.uk/research/rainbow/projects/alsarr/Perceptual_Model_for_Adaptive_Local_Shading_and_Refresh_Rate.pdf)
- The sharpening studies opened judged still frames or compressed clips without controlling motion speed. — [Hacking VMAF](https://arxiv.org/pdf/2107.04510); [RPO-AdaSharp](https://arxiv.org/pdf/2508.08794)

### Inferences
- Unstudied, as far as this round could establish:
  1. Visibility or preference for unsharp masking as a function of screen velocity (deg/s or px/frame), tracked vs. fixated.
  2. Visibility of overshoot/halos in moving vs. static images, and the effect of mask radius on it.
  3. Motion quality below 15 deg/s in the Cambridge models (flight-sim ground flow is about 1–6 deg/s at low altitude); predictions there are extrapolations.
  4. Gaze behaviour and retinal slip of desktop flight-sim users over streaming ground texture; how much optokinetic following stabilises it.
  5. Shimmer from a sharpened top mip level under minification weighed against the sharpness gain; no psychophysics found in either round.
  6. Sharpness perception on perspective-compressed, grazing-angle natural texture; all classic data are gratings, edges, optotypes or frontal images.
- The question is cheap to settle empirically for this project: an A/B capture of the same flight segment with and without sharpening at 30 and 60 fps would answer more than further literature search.

### Gaps
- "Unstudied" here means "not found in two rounds with restricted access". The TV-enhancement literature (peaking, LTI/CTI; SID and IEEE Transactions on Consumer Electronics) and the Philips/TU Eindhoven sharpness work sit on hosts that were not reachable, so a study may exist there.
