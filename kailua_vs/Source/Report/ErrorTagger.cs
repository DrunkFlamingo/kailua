﻿using System;
using System.Diagnostics;
using System.Collections.Generic;
using System.ComponentModel.Composition;
using Microsoft.VisualStudio.Text;
using Microsoft.VisualStudio.Text.Tagging;
using Microsoft.VisualStudio.Text.Adornments;
using Microsoft.VisualStudio.Shell;
using Microsoft.VisualStudio.Utilities;

namespace Kailua
{
    [Export(typeof(ITaggerProvider))]
    [ContentType("kailua")]
    [TagType(typeof(ErrorTag))]
    internal sealed class ErrorTaggerProvider : ITaggerProvider
    {
        [Import]
        internal IBufferTagAggregatorFactoryService aggregatorFactory = null;

        public ITagger<T> CreateTagger<T>(ITextBuffer buffer) where T : ITag
        {
            // one TokenTagger instance is unique to given ITextBuffer instance
            // (otherwise TagAggregator is unable to receive an event from it)
            var aggregator = aggregatorFactory.CreateTagAggregator<ReportTag>(buffer);
            return buffer.Properties.GetOrCreateSingletonProperty(delegate() { return new ErrorTagger(buffer, aggregator); }) as ITagger<T>;
        }
    }

    // also acts as a per-buffer error list provider when updated
    internal sealed class ErrorTagger : ITagger<ErrorTag>, IDisposable
    {
        internal ITextBuffer buffer;
        internal ITagAggregator<ReportTag> aggregator;
        internal ReportErrorListProvider errorListProvider;

        internal ErrorTagger(ITextBuffer buffer, ITagAggregator<ReportTag> aggregator)
        {
            this.buffer = buffer;
            this.aggregator = aggregator;
            this.errorListProvider = new ReportErrorListProvider();

            this.aggregator.TagsChanged += errorTagsChanged;
            this.aggregator.BatchedTagsChanged += batchedErrorTagsChanged;
        }

        internal String reportKindToErrorType(Native.ReportKind kind)
        {
            switch (kind)
            {
                case Native.ReportKind.Warning:
                    return PredefinedErrorTypeNames.Warning;
                case Native.ReportKind.Error:
                    return PredefinedErrorTypeNames.SyntaxError; // TODO
                case Native.ReportKind.Fatal:
                    return PredefinedErrorTypeNames.SyntaxError; // TODO
                default:
                    return null;
            }
        }

        internal void errorTagsChanged(object sender, TagsChangedEventArgs args)
        {
            // propagate the TagsChanged event
            if (this.TagsChanged != null)
            {
                var spans = args.Span.GetSpans(this.buffer);
                this.TagsChanged(sender, new SnapshotSpanEventArgs(spans[0]));
            }
        }

        internal void batchedErrorTagsChanged(object sender, BatchedTagsChangedEventArgs args)
        {
            // update the current error list to that of the current snapshot and most recent reports
            // (this should be batched, otherwise it will get recursively called!)
            // TODO actually, the line number can be calculated straight from Source itself, so we don't really need Snapshot
            var snapshot = this.buffer.CurrentSnapshot;
            var invalidatedSpan = new SnapshotSpan(snapshot, 0, snapshot.Length);
            this.errorListProvider.Tasks.Clear();
            foreach (var reportSpan in this.aggregator.GetTags(invalidatedSpan))
            {
                this.errorListProvider.AddReport(snapshot, reportSpan.Tag.Data, reportSpan.Tag.Path);
            }
            this.errorListProvider.Show();
        }

        public event EventHandler<SnapshotSpanEventArgs> TagsChanged;

        public IEnumerable<ITagSpan<ErrorTag>> GetTags(NormalizedSnapshotSpanCollection spans)
        {
            var snapshot = spans[0].Snapshot;

            foreach (var reportSpan in this.aggregator.GetTags(spans))
            {
                var span = reportSpan.Span.GetSpans(snapshot)[0];
                var errorType = reportKindToErrorType(reportSpan.Tag.Data.Kind);
                if (errorType != null)
                {
                    yield return new TagSpan<ErrorTag>(span, new ErrorTag(errorType, reportSpan.Tag.Data.Message));
                }
            }
        }

        public void Dispose()
        {
            this.errorListProvider.Dispose();
        }
    }
}