# Host the Edict website

This directory contains the assets for the explanation page at [../index.html](../index.html). The page is static HTML and CSS. It needs no build, JavaScript, external fonts, analytics, or application server.

## Preview locally

Open `docs/index.html` directly in a browser. To test HTTP delivery, run this from the repository root:

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory docs
```

Open <http://127.0.0.1:8000/>. Stop the server with Control-C.

## Publish with GitHub Pages

After the website files are committed to the branch you want to publish:

1. Push that branch to your GitHub repository.
2. Open **Settings → Pages**.
3. Select **Deploy from a branch**.
4. Select your publishing branch and the **/docs** folder.
5. Save and wait for GitHub's Pages deployment to finish.

GitHub supplies the published URL. A repository site normally appears at `https://OWNER.github.io/REPOSITORY/`. Relative asset URLs support that prefix and a custom domain.

The `docs/.nojekyll` file disables Jekyll processing. Publishing `/docs` also makes the repository's other public documentation files available at their paths.

These are deployment instructions. Creating the files does not enable Pages, change repository settings, push a branch, or publish the site.

See [GitHub's publishing-source instructions](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site).

## Host only the landing page elsewhere

Copy these files, preserving the layout:

```text
index.html
.nojekyll
site/
  style.css
  favicon.svg
```

Any static file host can serve this tree. No base URL, API keys, dependencies, or compilation step is required.

## Edit the explanation

- `docs/index.html` owns the content, source links, native disclosure sections, and metadata.
- `docs/site/style.css` owns the responsive layout and palette.
- `docs/site/favicon.svg` owns the small browser icon.
- Keep the code excerpt aligned with its linked Hello Echo source.
- Keep version-specific facts linked to exact revisions.
- Separate current behavior from proposals, including Lean proof generation.
- Keep private Reader paths, conversations, participant data, and vault contents out of this public site.

The page is an explanation and repository entry point. Technical contracts remain in their existing topic shelves.

## Check a content or style change

Open the page at desktop and narrow phone widths. Check the repository links, internal navigation, keyboard focus, and native disclosure controls. Check that long code scrolls inside its own panel without widening the page.

Disable JavaScript and confirm that the page and disclosure controls still work. Check reduced-motion preference and text/background contrast after palette changes. Test under a repository path prefix before publishing.

These browser checks do not establish compiler behavior or hosted Pages availability. The repository's normal verification and review requirements still apply before merging.
